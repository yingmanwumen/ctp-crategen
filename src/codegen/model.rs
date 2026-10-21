use heck::ToSnakeCase;

#[derive(Debug, Clone)]
pub struct Class {
    pub name: String,
    pub methods: Vec<Method>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub name: String,
    pub return_type: String,
    pub params: Vec<Param>,
    pub is_static: bool,
    pub is_virtual: bool,
    pub is_pure: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub cpp_type: String,
    pub default: Option<String>,
}

impl Class {
    pub fn codegen(&self) -> (String, String) {
        let (mut hpp, mut cpp) = (String::new(), String::new());
        for method in &self.methods {
            if method.is_static && method.name.contains("Create") {
                let (method_hpp, method_cpp) = method.factory_codegen(&self.name);
                hpp += &method_hpp;
                cpp += &method_cpp;
            }
        }
        (hpp, cpp)
    }
}

impl Method {
    pub fn factory_codegen(&self, class_name: &str) -> (String, String) {
        assert!(self.is_static);
        let factory_name = self.name.to_snake_case();
        let params = self
            .params
            .iter()
            .map(|param| param.codegen())
            .collect::<Vec<_>>()
            .join(", ");
        let args = self
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        let method_name = &self.name;
        let doc = self.doc.as_deref().unwrap_or_default();

        let hpp = format!(
            r#"
{doc}
void *{factory_name}({params});
"#
        );

        let cpp = format!(
            r#"
{doc}
void *{factory_name}({params}) {{
    return {class_name}::{method_name}({args});
}}
            "#
        );

        (hpp, cpp)
    }
}

impl Param {
    pub fn codegen(&self) -> String {
        format!("{} {}", self.cpp_type, self.name)
    }
}
