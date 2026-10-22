use heck::ToSnakeCase;

use crate::codegen::model::{RequestFieldKind, RequestStruct};

impl RequestStruct {
    pub fn rust_codegen(&self) -> String {
        let raw = &self.name;
        let name = raw.strip_prefix("CThostFtdc").unwrap_or(raw);
        let name = name.strip_suffix("Field").unwrap_or(name);
        let builder = format!("{name}Builder");
        let mut setters = String::new();
        for field in &self.fields {
            let field_name = &field.name;
            // Raw identifiers also cover Rust keywords in SDK field names.
            let setter = format!("r#{}", field_name.to_snake_case());
            let ty = &field.rust_type;
            let mut doc = field.doc.clone().unwrap_or_default();
            match field.kind {
                RequestFieldKind::Text => doc.push_str("\n    /// Writes UTF-8 bytes with a trailing NUL. Interior NUL and capacity errors are returned by `build()`."),
                RequestFieldKind::CharacterCode => doc.push_str("\n    /// Accepts a bindgen character constant (`u8`) and stores its byte in `c_char`."),
                RequestFieldKind::Value => {},
            }
            let (ty, assignment) = match field.kind {
                RequestFieldKind::Text => (
                    "&str",
                    format!(
                        "if self.error.is_none() {{\n            self.error = write_text(&mut self.raw.{field_name}, value, \"{raw}.{field_name}\").err();\n        }}"
                    ),
                ),
                RequestFieldKind::CharacterCode => (
                    "u8",
                    format!("self.raw.{field_name} = value as std::ffi::c_char;"),
                ),
                RequestFieldKind::Value => (ty.as_str(), format!("self.raw.{field_name} = value;")),
            };
            setters += &format!(
                r#"
{doc}
    pub fn {setter}(mut self, value: {ty}) -> Self {{
        {assignment}
        self
    }}
"#
            );
        }
        format!(
            r#"
pub use sys::{raw} as {name};

impl sys::{raw} {{
    pub fn builder() -> {builder} {{
        {builder}::default()
    }}
}}

/// Builds the original bindgen struct, without validating CTP business rules.
/// Unset fields are zero-initialized. `build()` reports the first text conversion error,
/// even if the same field is subsequently set again. Use raw fields for non-UTF-8 text.
#[derive(Default)]
pub struct {builder} {{
    raw: sys::{raw},
    error: Option<FieldError>,
}}

impl {builder} {{
{setters}
    pub fn build(self) -> Result<sys::{raw}, FieldError> {{
        match self.error {{
            Some(error) => Err(error),
            None => Ok(self.raw),
        }}
    }}
}}
"#
        )
    }
}
