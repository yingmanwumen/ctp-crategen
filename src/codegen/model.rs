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
            let (method_hpp, method_cpp) = method.codegen(&self.name);
            hpp += &method_hpp;
            cpp += &method_cpp;
        }
        (hpp, cpp)
    }
}

impl Method {
    pub fn codegen(&self, class_name: &str) -> (String, String) {
        if self.is_static {
            if self.name.contains("Create") {
                self.factory_codegen(class_name)
            } else {
                // Common static method
                self.static_method_codegen(class_name)
            }
        } else {
            (Default::default(), Default::default())
        }
    }

    fn static_method_codegen(&self, class_name: &str) -> (String, String) {
        assert!(self.is_static);

        let c_method_name = format!(
            "{}_{}",
            class_name.to_snake_case(),
            self.name.to_snake_case()
        );
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
        let return_type = &self.return_type;
        let doc = self.doc.as_deref().unwrap_or_default();

        let hpp = format!(
            r#"
{doc}
{return_type} {c_method_name}({params});
"#
        );

        let cpp = format!(
            r#"
{doc}
{return_type} {c_method_name}({params}) {{
    return {class_name}::{method_name}({args});
}}
            "#
        );

        (hpp, cpp)
    }

    fn factory_codegen(&self, class_name: &str) -> (String, String) {
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
