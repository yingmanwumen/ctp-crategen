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
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum RequestFieldKind {
    Value,
    Text,
    CharacterCode,
}

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

#[derive(Debug, Clone)]
pub struct ErrorCode {
    pub name: String,
    pub value: i32,
    pub prompt: String,
}
