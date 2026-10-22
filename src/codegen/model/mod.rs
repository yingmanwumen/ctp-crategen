mod cpp;
mod rust;

#[derive(Debug, Clone)]
pub struct RequestStruct {
    pub name: String,
    pub fields: Vec<RequestField>,
}

#[derive(Debug, Clone)]
pub struct RequestField {
    pub name: String,
    pub rust_type: String,
    pub kind: RequestFieldKind,
    pub length_field: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum RequestFieldKind {
    Value,
    Text,
    CharacterCode,
    CharacterCodeArray,
    Bytes,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub name: String,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub name: String,
    pub return_type: String,
    pub params: Vec<Param>,
    pub is_static: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub cpp_type: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ErrorCode {
    pub name: String,
    pub value: i32,
    pub prompt: String,
}
