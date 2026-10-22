use heck::ToUpperCamelCase;

use super::super::ErrorCode;

impl ErrorCode {
    pub fn rust_codegen(&self) -> (String, String, String) {
        let name = if self.name == "NONE" && self.value == 0 {
            String::from("Success")
        } else {
            self.name.to_upper_camel_case()
        };
        let value = self.value;
        let prompt = format!("{:?}", self.prompt);
        let variant = format!("    #[doc = {prompt}]\n    {name} = {value},\n");
        let from_code = format!("            {value} => Ok(Self::{name}),\n");
        let message = format!("            Self::{name} => {prompt},\n");
        (variant, from_code, message)
    }
}
