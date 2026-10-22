use super::super::ErrorCode;

impl ErrorCode {
    pub fn rust_codegen(&self) -> (String, String) {
        let name = &self.name;
        let value = self.value;
        let prompt = format!("{:?}", self.prompt);
        let constant = format!("pub const CTP_ERROR_{name}: i32 = {value};\n");
        let branch = format!("        {value} => Some({prompt}),\n");
        (constant, branch)
    }
}
