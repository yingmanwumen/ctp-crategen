use heck::ToSnakeCase;

use crate::codegen::model::{RequestFieldKind, RequestStruct};

impl RequestStruct {
    pub fn rust_codegen(&self) -> String {
        let raw = &self.name;
        let name = raw.strip_prefix("CThostFtdc").unwrap_or(raw);
        let name = name.strip_suffix("Field").unwrap_or(name);
        let builder = format!("{name}Builder");
        let mut setters = String::new();
        let mut defaults = String::new();
        for field in &self.fields {
            let field_name = &field.name;
            if field.rust_type == "sys::TThostFtdcCombHedgeFlagType" {
                defaults += &format!(
                    "        raw.{field_name}[0] = sys::THOST_FTDC_HF_Speculation as std::ffi::c_char;\n"
                );
            } else if field.rust_type == "sys::TThostFtdcHedgeFlagType" {
                defaults += &format!(
                    "        raw.{field_name} = sys::THOST_FTDC_HF_Speculation as std::ffi::c_char;\n"
                );
            }
            // The byte setter derives the actual length; do not expose a conflicting setter.
            if self
                .fields
                .iter()
                .any(|buffer| buffer.length_field.as_deref() == Some(field_name.as_str()))
            {
                continue;
            }
            // Raw identifiers also cover Rust keywords in SDK field names.
            let setter = format!("r#{}", field_name.to_snake_case());
            let ty = &field.rust_type;
            let mut doc = field.doc.clone().unwrap_or_default();
            match field.kind {
                RequestFieldKind::Text => doc.push_str("\n    /// Writes UTF-8 bytes with a trailing NUL. Interior NUL and capacity errors are returned by `build()`."),
                RequestFieldKind::CharacterCode => doc.push_str("\n    /// Accepts a bindgen character constant (`u8`) and stores its byte in `c_char`."),
                RequestFieldKind::CharacterCodeArray => doc.push_str("\n    /// Sets the first flag and clears the remaining SDK array slots."),
                RequestFieldKind::Bytes => {
                    doc.push_str("\n    /// Copies binary data without NUL termination. Capacity errors are returned by `build()`.");
                    if let Some(length) = &field.length_field {
                        doc += &format!("\n    /// Automatically sets `{length}` to the byte length.");
                    }
                }
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
                RequestFieldKind::CharacterCodeArray => (
                    "u8",
                    format!(
                        "self.raw.{field_name}.fill(0);\n        self.raw.{field_name}[0] = value as std::ffi::c_char;"
                    ),
                ),
                RequestFieldKind::Bytes => {
                    let call = format!(
                        "write_bytes(&mut self.raw.{field_name}, value, \"{raw}.{field_name}\")"
                    );
                    let conversion = match &field.length_field {
                        Some(length) => format!(
                            "match {call} {{\n                Ok(length) => self.raw.{length} = length,\n                Err(error) => self.error = Some(error),\n            }}"
                        ),
                        None => format!("self.error = {call}.err();"),
                    };
                    (
                        "&[u8]",
                        format!("if self.error.is_none() {{\n            {conversion}\n        }}"),
                    )
                }
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
        let binding = if defaults.is_empty() {
            "raw"
        } else {
            "mut raw"
        };
        format!(
            r#"
pub use sys::{raw} as {name};

impl sys::{raw} {{
    pub fn builder() -> {builder} {{
        {builder}::default()
    }}
}}

/// Builds the original bindgen struct, without validating CTP business rules.
/// Unset fields are zero-initialized except hedge flags, which default to speculation.
/// `build()` reports the first field conversion error,
/// even if the same field is subsequently set again. Use raw fields for non-UTF-8 text.
pub struct {builder} {{
    raw: sys::{raw},
    error: Option<FieldError>,
}}

impl Default for {builder} {{
    fn default() -> Self {{
        let {binding} = sys::{raw}::default();
{defaults}        Self {{ raw, error: None }}
    }}
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
