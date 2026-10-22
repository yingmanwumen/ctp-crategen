use heck::ToSnakeCase;

use super::rust_value_type;
use crate::codegen::model::{Class, Param};

impl Class {
    pub fn rust_spi_codegen(&self) -> anyhow::Result<String> {
        let name = self.name.strip_prefix("CThostFtdc").unwrap_or(&self.name);
        let owner = format!("{name}Owner");
        let context = format!("{name}Context");
        let kind = if name == "MdSpi" { "md" } else { "trader" };
        let callbacks_type = format!("{}Callbacks", self.name);
        let mut methods = String::new();
        let mut callbacks = String::new();
        let mut trampolines = String::new();

        for method in &self.methods {
            anyhow::ensure!(
                method.return_type.trim() == "void" && !method.is_static,
                "unsupported SPI method: {}::{}",
                self.name,
                method.name
            );
            let method_name = method.name.to_snake_case();
            let doc = method.doc.as_deref().unwrap_or_default();
            let mut trait_params = vec![String::from("&self")];
            let mut raw_params = vec![String::from("userdata: *mut c_void")];
            let mut args = Vec::new();
            for param in &method.params {
                let (declaration, raw_declaration, arg) = param.rust_spi_codegen()?;
                trait_params.push(declaration);
                raw_params.push(raw_declaration);
                args.push(arg);
            }
            let trait_params = trait_params.join(", ");
            let raw_params = raw_params.join(", ");
            let args = args.join(", ");
            methods += &format!(
                r#"
{doc}
    fn {method_name}({trait_params}) {{}}
"#
            );
            callbacks += &format!("            {method_name}: Some(Self::{method_name}),\n");
            trampolines += &format!(
                r#"
    unsafe extern "C" fn {method_name}({raw_params}) {{
        // The API owns this stable allocation until Release has joined the SDK threads.
        let context = unsafe {{ &*userdata.cast::<{context}>() }};
        context.handler.{method_name}({args});
    }}
"#
            );
        }

        Ok(format!(
            r#"
/// Receives callbacks directly on SDK threads; methods must return promptly.
///
/// Borrowed data is valid only for the callback duration. Copy it to retain it.
/// No serialization, queueing or panic recovery is added by this wrapper.
/// An uncaught panic aborts the process at the non-unwinding C ABI boundary.
/// Prepare all handler state before calling `build()`: callbacks may precede its return.
#[allow(unused_variables)]
pub trait {name}: Send + Sync + 'static {{
{methods}}}

struct {context} {{
    handler: Box<dyn {name}>,
}}

pub(crate) struct {owner} {{
    handle: NonNull<c_void>,
    // Owns a Box allocation exposed to C++; reconstructed only after SDK shutdown.
    context: NonNull<{context}>,
}}

impl {owner} {{
    const CALLBACKS: sys::{callbacks_type} = sys::{callbacks_type} {{
{callbacks}    }};

    pub(crate) fn new(handler: Box<dyn {name}>) -> Result<Self, Error> {{
        let context = NonNull::from(Box::leak(Box::new({context} {{ handler }})));
        // The C++ bridge copies the table and retains userdata, not the table reference.
        let raw = unsafe {{ sys::create_{kind}_spi(&Self::CALLBACKS, context.as_ptr().cast()) }};
        match NonNull::new(raw) {{
            Some(handle) => Ok(Self {{ handle, context }}),
            None => {{
                // Construction failed before registration; no callback can access the context.
                unsafe {{ drop(Box::from_raw(context.as_ptr())); }}
                Err(Error::CreateFailed("{name}"))
            }}
        }}
    }}

    pub(crate) fn raw(&self) -> *mut c_void {{
        self.handle.as_ptr()
    }}
{trampolines}}}

impl Drop for {owner} {{
    fn drop(&mut self) {{
        // The enclosing API calls Release first; no SDK callback can still use this object.
        unsafe {{
            sys::destroy_{kind}_spi(self.handle.as_ptr());
            drop(Box::from_raw(self.context.as_ptr()));
        }}
    }}
}}
"#
        ))
    }
}

impl Param {
    fn rust_spi_codegen(&self) -> anyhow::Result<(String, String, String)> {
        let name = self.name.to_snake_case();
        let compact = self.cpp_type.split_whitespace().collect::<String>();
        let (trait_type, raw_type, arg) = if let Some(pointee) = compact.strip_suffix('*') {
            let (qualifier, pointee) = match pointee.strip_prefix("const") {
                Some(pointee) => ("const", pointee),
                None => ("mut", pointee),
            };
            anyhow::ensure!(
                pointee.starts_with("CThostFtdc") && pointee.ends_with("Field"),
                "unsupported SPI pointer: {}",
                self.cpp_type
            );
            (
                format!("Option<&sys::{pointee}>"),
                format!("*{qualifier} sys::{pointee}"),
                // CTP owns the data and keeps it valid for the synchronous callback.
                format!("unsafe {{ {name}.as_ref() }}"),
            )
        } else {
            let ty = rust_value_type(&self.cpp_type)?;
            (ty.clone(), ty, name.clone())
        };
        Ok((
            format!("{name}: {trait_type}"),
            format!("{name}: {raw_type}"),
            arg,
        ))
    }
}
