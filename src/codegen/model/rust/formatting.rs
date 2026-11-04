use crate::codegen::model::{DataStruct, FieldKind};

impl DataStruct {
    pub fn rust_formatting_codegen(&self) -> String {
        let name = &self.name;
        let mut fields = String::new();
        for field in &self.fields {
            let field_name = &field.name;
            let value = format!("self.r#{field_name}");
            let formatted = match field.kind {
                FieldKind::Text => format!("crate::decode_ctp_text(&{value})"),
                FieldKind::CharacterCode => format!("({value} as u8 as char)"),
                FieldKind::CharacterCodeArray => format!("CharacterCodes(&{value})"),
                FieldKind::Bytes => match &field.length_field {
                    Some(length) => {
                        format!("Bytes(bytes_with_length(&{value}, self.r#{length}))")
                    }
                    None => format!("Bytes(&{value})"),
                },
                FieldKind::Value => value,
            };
            fields += &format!("        debug.field({field_name:?}, &{formatted});\n");
        }
        format!(
            r#"
impl std::fmt::Debug for crate::sys::{name} {{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
        let mut debug = f.debug_struct("{name}");
{fields}        debug.finish()
    }}
}}

impl std::fmt::Display for crate::sys::{name} {{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
        std::fmt::Debug::fmt(self, f)
    }}
}}
"#
        )
    }
}
