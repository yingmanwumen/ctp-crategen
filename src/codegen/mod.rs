//! Code generation pipeline for converting validated CTP SDK data into crates.

mod model;
mod parser;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use bindgen::Bindings;
use clang::{Clang, Index};

use crate::{
    codegen::{model::Class, parser::Parser},
    sdk::{CLASSES, Sdk},
};

const BRIDGE_CPP_NAME: &str = "ctp_brdige.cpp";
const BRIDGE_HPP_NAME: &str = "ctp_brdige.hpp";

/// 1. Generate C bridge code
/// 2. Compile ctp_c.so
/// 3. Rust bindgen
/// 4. Combine them into a crate/module
pub struct Codegen {
    output_dir: PathBuf,
}

impl Codegen {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    pub fn generate(&self, sdk: &Sdk) -> anyhow::Result<()> {
        let classes = self.parse_classes(sdk)?;
        let (hpp, cpp) = self.cpp_bridge_codegen(classes)?;

        Self::write_file(self.output_dir.join(BRIDGE_HPP_NAME), &hpp)?;
        Self::write_file(self.output_dir.join(BRIDGE_CPP_NAME), &cpp)?;

        let rust_binding = self.rust_bindgen(sdk, &hpp)?;
        rust_binding.write_to_file(self.output_dir.join("lib.rs"))?;
        Ok(())
    }

    fn parse_classes(&self, sdk: &Sdk) -> anyhow::Result<HashMap<&str, Class>> {
        let clang = Clang::new().map_err(|e| anyhow::anyhow!(e))?;
        let index = Index::new(&clang, false, false);
        let parser = Parser::new(sdk, &index)?;

        CLASSES
            .iter()
            .map(|&class_name| parser.parse(class_name).map(|res| (class_name, res)))
            .collect::<anyhow::Result<HashMap<_, _>>>()
    }

    /// Generate .hpp & .cpp
    fn cpp_bridge_codegen(
        &self,
        classes: HashMap<&str, Class>,
    ) -> anyhow::Result<(String, String)> {
        let mut hpp = String::from(
            r#"
#pragma once

#include <stdbool.h>

#include "ThostFtdcUserApiStruct.h"
#include "ThostFtdcUserApiDataType.h"

#ifdef __cplusplus
extern "C" {
#endif
"#,
        );
        let mut cpp = format!(
            r#"
#include "{BRIDGE_HPP_NAME}"
#include "ThostFtdcTraderApi.h"
#include "ThostFtdcMdApi.h"
"#
        );
        for class in classes.values() {
            let (class_hpp, class_cpp) = class.codegen();
            hpp += &class_hpp;
            cpp += &class_cpp;
        }
        hpp += r#"
#ifdef __cplusplus
}
#endif
"#;
        Ok((hpp, cpp))
    }

    fn rust_bindgen(&self, sdk: &Sdk, wrapper_hpp: &str) -> anyhow::Result<Bindings> {
        Ok(bindgen::Builder::default()
            .clang_args(["-x", "c++"])
            .header_contents(
                "ThostFtdcUserApiStruct.h",
                sdk.get_header("ThostFtdcUserApiStruct.h").unwrap(),
            )
            .header_contents(
                "ThostFtdcUserApiDataType.h",
                sdk.get_header("ThostFtdcUserApiDataType.h").unwrap(),
            )
            .header_contents(BRIDGE_HPP_NAME, wrapper_hpp)
            .generate()?)
    }

    fn write_file(path: impl AsRef<Path>, content: &str) -> std::io::Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)
    }
}
