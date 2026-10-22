use std::{collections::HashSet, path::PathBuf};

use anyhow::Context;

use clang::{
    Entity,
    EntityKind::{self},
    EntityVisitResult, Index, TranslationUnit, Unsaved,
};

use crate::{
    codegen::model::{Class, ErrorCode, Method, Param},
    sdk::{HEADERS, Sdk},
};

pub fn parse_error_codes(text: &str) -> anyhow::Result<Vec<ErrorCode>> {
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .context("failed to parse error.xml")?;
    let root = document.root_element();
    anyhow::ensure!(
        root.has_tag_name("errors"),
        "error.xml root must be <errors>"
    );

    let mut errors = Vec::new();
    let mut names = HashSet::new();
    let mut values = HashSet::new();
    for node in root.children().filter(|node| node.is_element()) {
        anyhow::ensure!(
            node.has_tag_name("error"),
            "unexpected element in error.xml: {}",
            node.tag_name().name()
        );
        let name = node.attribute("id").context("error.xml entry missing id")?;
        anyhow::ensure!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
            "invalid error.xml id: {name}"
        );
        let value = node
            .attribute("value")
            .with_context(|| format!("error.xml entry {name} missing value"))?;
        let value = if let Some(hex) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        {
            i32::from_str_radix(hex, 16)
        } else {
            value.parse::<i32>()
        }
        .with_context(|| format!("invalid error.xml value for {name}: {value}"))?;
        let prompt = node
            .attribute("prompt")
            .with_context(|| format!("error.xml entry {name} missing prompt"))?;
        anyhow::ensure!(names.insert(name), "duplicate error.xml id: {name}");
        anyhow::ensure!(values.insert(value), "duplicate error.xml value: {value}");
        errors.push(ErrorCode {
            name: name.to_owned(),
            value,
            prompt: prompt.to_owned(),
        });
    }
    Ok(errors)
}

pub struct Parser<'a> {
    tu: TranslationUnit<'a>,
    sdk: &'a Sdk,
    virtual_root: PathBuf,
}

impl<'a> Parser<'a> {
    pub fn new(sdk: &'a Sdk, index: &'a Index<'a>) -> anyhow::Result<Self> {
        let virtual_root = PathBuf::from("/ctp-crategen-virtual");

        let header_paths: Vec<_> = HEADERS
            .iter()
            .map(|header| virtual_root.join(header))
            .collect();
        let unsaves: Vec<_> = HEADERS
            .iter()
            .zip(&header_paths)
            .map(|(header, path)| {
                let contents = sdk
                    .get_header(header)
                    .ok_or_else(|| anyhow::anyhow!("missing SDK header: {header}"))?;
                Ok(Unsaved::new(path, contents))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let wrapper_content = header_paths
            .iter()
            .map(|path| format!("#include \"{}\"", path.display()))
            .collect::<Vec<_>>()
            .join("\n");

        let wrapper_path = virtual_root.join("__ctp_wrapper.cpp");
        let tu = index
            .parser(&wrapper_path)
            .arguments(&["-x", "c++", "-fsyntax-only"])
            .unsaved(&[unsaves, vec![Unsaved::new(wrapper_path, wrapper_content)]].concat())
            .parse()?;

        for diagnostic in tu.get_diagnostics() {
            if diagnostic.get_severity() >= clang::diagnostic::Severity::Error {
                anyhow::bail!("libclang parse failed: {diagnostic}");
            }
        }

        Ok(Self {
            tu,
            sdk,
            virtual_root,
        })
    }

    pub fn parse(&self, class_name: &str) -> anyhow::Result<Class> {
        let class = self
            .look_for_complete_class(class_name)
            .ok_or_else(|| anyhow::anyhow!("class definition not found: {class_name}"))?;

        // for method in self.look_for_common_methods(class) {
        //     let return_type = method
        //         .get_result_type()
        //         .map(|ty| ty.get_display_name())
        //         .unwrap_or_else(|| String::from("void"));
        //     let params = method.get_arguments().unwrap_or_default();
        //     params.into_iter().map(|param| {
        //         let name = param.get_name().unwrap();
        //         let cpp_type = param
        //             .get_type()
        //             .map(|ty| ty.get_display_name())
        //             .unwrap_or_else(|| String::from("void"));
        //     });
        // }

        let methods = self
            .look_for_common_methods(class)
            .into_iter()
            .map(|method| {
                let name = method.get_name().unwrap();
                let return_type = method
                    .get_result_type()
                    .map(|ty| ty.get_display_name())
                    .unwrap_or_else(|| String::from("void"));

                let params = method
                    .get_arguments()
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .map(|(index, param)| {
                        let name = param.get_name().unwrap_or_else(|| format!("arg{index}"));
                        let cpp_type = param
                            .get_type()
                            .map(|ty| ty.get_display_name())
                            .unwrap_or_default();
                        let default = self.get_default_value(param);

                        Param {
                            name,
                            cpp_type,
                            default,
                        }
                    })
                    .collect();

                Method {
                    name,
                    return_type,
                    params,
                    is_static: method.is_static_method(),
                    is_virtual: method.is_virtual_method(),
                    is_pure: method.is_pure_virtual_method(),
                    doc: method.get_comment(),
                }
            })
            .collect();

        Ok(Class {
            name: String::from(class_name),
            methods,
            doc: class.get_comment(),
        })
    }

    fn look_for_complete_class(&self, class_name: &str) -> Option<Entity<'_>> {
        let root = self.tu.get_entity();
        let mut res = None;

        root.visit_children(|child, _| {
            if child.get_kind() == EntityKind::ClassDecl
                && child.get_name().as_deref() == Some(class_name)
                && child.is_definition()
            {
                res = Some(child);
                EntityVisitResult::Break
            } else {
                EntityVisitResult::Recurse
            }
        });
        res
    }

    fn look_for_common_methods(&self, class: Entity<'a>) -> Vec<Entity<'a>> {
        let mut methods = Vec::new();

        class.visit_children(|child, _| {
            if child.get_kind() != EntityKind::Method {
                return EntityVisitResult::Continue;
            }

            if let Some(name) = child.get_name() {
                if name.starts_with("operator") {
                    return EntityVisitResult::Continue;
                }
            } else {
                return EntityVisitResult::Continue;
            }
            methods.push(child);

            EntityVisitResult::Continue
        });

        methods
    }

    fn source_span(&self, entity: Entity) -> Option<(PathBuf, usize, usize)> {
        let range = entity.get_range()?;
        let start = range.get_start().get_file_location();
        let end = range.get_end().get_file_location();
        let file = start.file?.get_path();

        Some((file, start.offset as usize, end.offset as usize))
    }

    fn get_default_value(&self, param: Entity) -> Option<String> {
        let (file, begin, end) = self.source_span(param)?;
        let file = file.strip_prefix(&self.virtual_root).ok()?;

        let code = self.sdk.get_header(&file.to_string_lossy()).unwrap();
        let snippet = code.get(begin..end)?;
        let (_, default) = snippet.split_once('=')?;
        (!default.is_empty()).then_some(String::from(default))
    }
}
