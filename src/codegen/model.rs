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
        if self.name.contains("Api") {
            self.api_codegen()
        } else {
            self.spi_codegen()
        }
    }

    fn callback_table_codegen(&self) -> (String, String) {
        let mut callback_table = String::new();
        for method in &self.methods {
            let params = [
                vec![String::from("void *userdata")],
                method.params.iter().map(|param| param.codegen()).collect(),
            ]
            .concat()
            .join(", ");
            callback_table += &format!("void (*{})({});\n", method.name.to_snake_case(), params);
        }
        let callback_table_name = format!("{}Callbacks", self.name);
        let hpp = format!(
            r#"
typedef struct {callback_table_name} {{
{callback_table}
}} {callback_table_name};
"#
        );
        (hpp, Default::default())
    }

    fn impl_class_codegen(&self) -> (String, String) {
        let callback_table_name = format!("{}Callbacks", self.name);
        let mut overrides = String::new();
        for method in &self.methods {
            let params = method
                .params
                .iter()
                .map(|param| param.codegen())
                .collect::<Vec<_>>()
                .join(", ");
            let args = [
                vec!["m_userdata"],
                method
                    .params
                    .iter()
                    .map(|param| param.name.as_ref())
                    .collect::<Vec<_>>(),
            ]
            .concat()
            .join(", ");
            let callback_name = &method.name;
            let c_callback_name = method.name.to_snake_case();
            overrides += &format!(
                r#"
void {callback_name}({params}) override {{
    if (m_callbacks.{c_callback_name}) {{
        m_callbacks.{c_callback_name}({args});
    }}
}}
"#
            );
        }

        let impl_name = format!("{}Impl", self.name);
        let class_name = &self.name;
        let cpp = format!(
            r#"
class {impl_name} final: public {class_name} {{
    {callback_table_name} m_callbacks;
    void *m_userdata;
public:
    {impl_name}(const {callback_table_name}& callbacks, void *userdata): m_callbacks(callbacks), m_userdata(userdata) {{}}
    {overrides}
}};
"#
        );
        (Default::default(), cpp)
    }

    fn impl_class_wrapper_codegen(&self) -> (String, String) {
        let type_name = if self.name.contains("Md") {
            "md"
        } else {
            "trader"
        };
        let create_wrapper_name = format!("create_{type_name}_spi");
        let destroy_wrapper_name = format!("destroy_{type_name}_spi");
        let callback_table_name = format!("{}Callbacks", self.name);
        let impl_name = format!("{}Impl", self.name);
        let class_name = &self.name;
        let hpp = format!(
            r#"
void *{create_wrapper_name}(const {callback_table_name} *callbacks, void *userdata);
void {destroy_wrapper_name}(void *spi_handle);
"#
        );
        let cpp = format!(
            r#"
void *{create_wrapper_name}(const {callback_table_name} *callbacks, void *userdata) {{
    if (!callbacks) {{
        return nullptr;
    }}
    auto *object = new (std::nothrow) {impl_name}(*callbacks, userdata);
    {class_name} *base = object;
    return static_cast<void*>(base);
}}

void {destroy_wrapper_name}(void *spi_handle) {{
    auto *base = static_cast<{class_name}*>(spi_handle);
    auto *object = static_cast<{impl_name}*>(base);
    delete object;
}}
"#
        );
        (hpp, cpp)
    }

    pub fn spi_codegen(&self) -> (String, String) {
        let (mut hpp, mut cpp) = (String::new(), String::new());

        let (callback_table_hpp, callback_table_cpp) = self.callback_table_codegen();
        let (impl_class_hpp, impl_class_cpp) = self.impl_class_codegen();
        let (impl_class_wrapper_hpp, impl_class_wrapper_cpp) = self.impl_class_wrapper_codegen();

        hpp += &callback_table_hpp;
        hpp += &impl_class_hpp;
        hpp += &impl_class_wrapper_hpp;

        cpp += &callback_table_cpp;
        cpp += &impl_class_cpp;
        cpp += &impl_class_wrapper_cpp;

        (hpp, cpp)
    }

    pub fn api_codegen(&self) -> (String, String) {
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
            if class_name.contains("Spi") {
                (Default::default(), Default::default())
            } else if self.name.contains("RegisterSpi") {
                self.register_spi_codegn(class_name)
            } else {
                self.common_codegen(class_name)
            }
        }
    }

    fn register_spi_codegn(&self, class_name: &str) -> (String, String) {
        assert!(!self.is_static);

        let c_method_name = format!(
            "{}_{}",
            class_name.to_snake_case(),
            self.name.to_snake_case()
        );
        let params = ["void *handle", "void *spi_handle"].join(", ");
        let spi_name = &self.params[0].cpp_type;

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
    auto *api = static_cast<{class_name}*>(handle);
    auto *spi = static_cast<{spi_name}>(spi_handle);
    return api->{method_name}(spi);
}}
"#
        );
        (hpp, cpp)
    }

    fn common_codegen(&self, class_name: &str) -> (String, String) {
        assert!(!self.is_static);

        let c_method_name = format!(
            "{}_{}",
            class_name.to_snake_case(),
            self.name.to_snake_case()
        );
        let params = [
            vec![String::from("void *handle")],
            self.params
                .iter()
                .map(|param| param.codegen())
                .collect::<Vec<_>>(),
        ]
        .concat()
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
    auto *api = static_cast<{class_name}*>(handle);
    return api->{method_name}({args});
}}
"#
        );
        (hpp, cpp)
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
        let cpp_type = match self.cpp_type.as_ref() {
            "char *[]" => "char**",
            _ => &self.cpp_type,
        };
        format!("{} {}", cpp_type, self.name)
    }
}
