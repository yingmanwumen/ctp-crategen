use heck::ToSnakeCase;

mod error;

use super::{Class, Method, Param};

impl Class {
    fn rust_builder_codegen(&self, factory: &Method) -> anyhow::Result<String> {
        let api_name = self.name.strip_prefix("CThostFtdc").unwrap_or(&self.name);
        let builder_name = format!("{api_name}Builder");
        let prefix = self.name.to_snake_case();

        let mut fields = String::new();
        let mut defaults = String::new();
        let mut setters = String::new();
        let mut setup = String::new();
        let mut args = Vec::new();
        for param in &factory.params {
            let name = match param.name.as_str() {
                "pszFlowPath" => String::from("flow_path"),
                "bIsUsingUdp" => String::from("using_udp"),
                "bIsMulticast" => String::from("multicast"),
                _ => param.name.to_snake_case(),
            };
            if param.name == "pszFlowPath" {
                fields += &format!("    {name}: PathBuf,\n");
                defaults += &format!("            {name}: PathBuf::new(),\n");
                setters += &format!(
                    r#"
    pub fn {name}(mut self, path: impl Into<PathBuf>) -> Self {{
        self.{name} = path.into();
        self
    }}
"#
                );
                setup += &format!(
                    r#"
        let {name} = CString::new(self.{name}.as_os_str().as_encoded_bytes())?;
"#
                );
                args.push(format!("{name}.as_ptr()"));
            } else {
                let rust_type = rust_value_type(&param.cpp_type)?;
                let default = match param.default.as_deref().map(str::trim) {
                    Some("true") => "true",
                    Some("false") => "false",
                    _ => "Default::default()",
                };
                fields += &format!("    {name}: {rust_type},\n");
                defaults += &format!("            {name}: {default},\n");
                setters += &format!(
                    r#"
    pub fn {name}(mut self, value: {rust_type}) -> Self {{
        self.{name} = value;
        self
    }}
"#
                );
                args.push(format!("self.{name}"));
            }
        }

        let mut configuration = String::new();
        for method in self
            .methods
            .iter()
            .filter(|method| method.is_configuration())
        {
            if method.params.len() > 1 {
                let params = method
                    .params
                    .iter()
                    .map(|param| param.rust_codegen().map(|(declaration, _, _)| declaration))
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .join(", ");
                let types = method
                    .params
                    .iter()
                    .map(|param| rust_value_type(&param.cpp_type))
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .join(", ");
                let args = method
                    .params
                    .iter()
                    .map(|param| param.name.to_snake_case())
                    .collect::<Vec<_>>()
                    .join(", ");
                let name = method.name.to_snake_case();
                fields += &format!("    {name}: Option<({types})>,\n");
                defaults += &format!("            {name}: None,\n");
                setters += &format!(
                    r#"
    pub fn {name}(mut self, {params}) -> Self {{
        self.{name} = Some(({args}));
        self
    }}
"#
                );
                configuration += &format!(
                    r#"
        if let Some(({args})) = self.{name} {{
            unsafe {{ sys::{prefix}_{name}(handle.as_ptr(), {args}); }}
        }}
"#
                );
                continue;
            }
            anyhow::ensure!(
                method.params.len() == 1,
                "unsupported configuration: {}",
                method.name
            );
            let param = &method.params[0];
            let name = method.name.to_snake_case();
            let param_name = param.name.to_snake_case();
            let (declaration, _, _) = param.rust_codegen()?;
            let assignment;
            if param.is_string() {
                fields += &format!("    {name}: Vec<String>,\n");
                defaults += &format!("            {name}: Vec::new(),\n");
                assignment = format!("self.{name}.push({param_name}.to_owned());");
                setup += &format!(
                    r#"
        let mut {name} = self.{name}.iter()
            .map(|value| c_string(value))
            .collect::<Result<Vec<_>, _>>()?;
"#
                );
                configuration += &format!(
                    r#"
        for value in &mut {name} {{
            unsafe {{ sys::{prefix}_{name}(handle.as_ptr(), value.as_mut_ptr()); }}
        }}
"#
                );
            } else {
                let (rust_type, value) = if let Some(ty) = param.cpp_type.trim().strip_suffix('*') {
                    (format!("sys::{}", ty.trim()), format!("*{param_name}"))
                } else {
                    (rust_value_type(&param.cpp_type)?, param_name.clone())
                };
                let arg = if param.cpp_type.trim().ends_with('*') {
                    "value"
                } else {
                    "*value"
                };
                fields += &format!("    {name}: Option<{rust_type}>,\n");
                defaults += &format!("            {name}: None,\n");
                assignment = format!("self.{name} = Some({value});");
                configuration += &format!(
                    r#"
        if let Some(value) = self.{name}.as_mut() {{
            unsafe {{ sys::{prefix}_{name}(handle.as_ptr(), {arg}); }}
        }}
"#
                );
            }
            setters += &format!(
                r#"
    pub fn {name}(mut self, {declaration}) -> Self {{
        {assignment}
        self
    }}
"#
            );
        }

        let factory_name = factory.name.to_snake_case();
        let args = args.join(", ");
        Ok(format!(
            r#"
pub struct {builder_name} {{
{fields}}}

impl Default for {builder_name} {{
    fn default() -> Self {{
        Self {{
{defaults}        }}
    }}
}}

impl {builder_name} {{
{setters}
    /// Creates, configures and starts the SDK; connection and login remain asynchronous.
    pub fn build(mut self) -> Result<{api_name}, Error> {{
{setup}        let raw = unsafe {{ sys::{factory_name}({args}) }};
        let handle = NonNull::new(raw).ok_or(Error::CreateFailed("{api_name}"))?;
{configuration}
        unsafe {{ sys::{prefix}_init(handle.as_ptr()); }}
        Ok({api_name} {{
            handle: Some(handle),
        }})
    }}
}}
"#
        ))
    }

    pub fn rust_codegen(&self) -> anyhow::Result<String> {
        let api_name = self.name.strip_prefix("CThostFtdc").unwrap_or(&self.name);
        let builder_name = format!("{api_name}Builder");
        let release_name = format!("{}_release", self.name.to_snake_case());
        let factory = self
            .methods
            .iter()
            .find(|method| method.is_static && method.name.starts_with("Create"))
            .ok_or_else(|| anyhow::anyhow!("missing factory for {}", self.name))?;
        let builder = self.rust_builder_codegen(factory)?;
        let mut methods = String::new();
        for method in &self.methods {
            if method.name != factory.name
                && !matches!(method.name.as_str(), "RegisterSpi" | "Init" | "Release")
                && !method.is_configuration()
            {
                methods += &method.rust_codegen(&self.name)?;
            }
        }

        Ok(format!(
            r#"{builder}
pub struct {api_name} {{
    handle: Option<NonNull<c_void>>,
}}

impl {api_name} {{
    pub fn builder() -> {builder_name} {{
        {builder_name}::default()
    }}

    pub fn release(mut self) {{
        self.release_inner();
    }}

    fn raw(&self) -> *mut c_void {{
        self.handle.expect("API already released").as_ptr()
    }}

    fn release_inner(&mut self) {{
        if let Some(handle) = self.handle.take() {{
            unsafe {{
                sys::{release_name}(handle.as_ptr());
            }}
        }}
    }}
{methods}}}

impl Drop for {api_name} {{
    fn drop(&mut self) {{
        self.release_inner();
    }}
}}
"#
        ))
    }
}

impl Method {
    fn is_configuration(&self) -> bool {
        matches!(
            self.name.as_str(),
            "RegisterFront"
                | "RegisterNameServer"
                | "RegisterFensUserInfo"
                | "SubscribePrivateTopic"
                | "SubscribePublicTopic"
        )
    }

    fn rust_codegen(&self, class_name: &str) -> anyhow::Result<String> {
        let name = match self.name.as_str() {
            "GetApiVersion" => String::from("api_version"),
            "GetTradingDay" => String::from("trading_day"),
            "UnSubscribeMarketData" => String::from("unsubscribe_market_data"),
            "UnSubscribeForQuoteRsp" => String::from("unsubscribe_for_quote_rsp"),
            _ => self.name.to_snake_case(),
        };
        let c_method_name = format!(
            "{}_{}",
            class_name.to_snake_case(),
            self.name.to_snake_case()
        );
        let mut params = Vec::new();
        let mut args = Vec::new();
        if !self.is_static {
            params.push(String::from(if self.return_type == "void" {
                "&mut self"
            } else {
                "&self"
            }));
            args.push(String::from("self.raw()"));
        }

        let (method_params, setup, method_args) = if self.params.iter().any(Param::is_string_array)
        {
            self.rust_array_params_codegen()?
        } else {
            self.rust_params_codegen()?
        };
        params.extend(method_params);
        args.extend(method_args);
        let params = params.join(", ");
        let args = args.join(", ");
        let call = format!("unsafe {{ sys::{c_method_name}({args}) }}");
        let fallible = self.params.iter().any(Param::is_string);
        let (return_type, body) = self.rust_return_codegen(call, fallible)?;
        let doc = self.doc.as_deref().unwrap_or_default();

        Ok(format!(
            r#"
{doc}
    pub fn {name}({params}) -> {return_type} {{
{setup}
        {body}
    }}
"#
        ))
    }

    fn rust_params_codegen(&self) -> anyhow::Result<(Vec<String>, String, Vec<String>)> {
        let mut params = Vec::new();
        let mut setup = String::new();
        let mut args = Vec::new();
        for param in &self.params {
            let (declaration, conversion, arg) = param.rust_codegen()?;
            params.push(declaration);
            setup += &conversion;
            args.push(arg);
        }
        Ok((params, setup, args))
    }

    fn rust_array_params_codegen(&self) -> anyhow::Result<(Vec<String>, String, Vec<String>)> {
        anyhow::ensure!(
            self.params.len() == 2
                && self.params[0].is_string_array()
                && self.params[1].cpp_type.trim() == "int",
            "unsupported string array parameters: {}",
            self.name
        );
        let name = self.params[0].name.to_snake_case();
        let params = vec![format!("{name}: &[&str]")];
        let setup = format!(
            r#"
        let count = i32::try_from({name}.len()).map_err(|_| Error::TooManyInstruments)?;
        let mut buffers = {name}.iter()
            .map(|value| c_string(value))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers = buffers.iter_mut()
            .map(|value| value.as_mut_ptr())
            .collect::<Vec<_>>();
"#
        );
        let args = vec![String::from("pointers.as_mut_ptr()"), String::from("count")];
        Ok((params, setup, args))
    }

    fn rust_return_codegen(
        &self,
        call: String,
        fallible: bool,
    ) -> anyhow::Result<(String, String)> {
        Ok(match self.return_type.trim() {
            "void" if fallible => (
                String::from("Result<(), Error>"),
                format!(
                    r#"{call};
        Ok(())"#
                ),
            ),
            "void" => (String::from("()"), format!("{call};")),
            "int" if self.name == "Join" => (String::from("i32"), call),
            "int" if fallible => (
                String::from("Result<(), Error>"),
                format!("request_result({call}).map_err(Error::from)"),
            ),
            "int" => (
                String::from("Result<(), RequestError>"),
                format!("request_result({call})"),
            ),
            "const char *" | "const char*" => (
                String::from("String"),
                format!(
                    r#"let value = {call};
        if value.is_null() {{
            String::new()
        }} else {{
            unsafe {{ CStr::from_ptr(value) }}.to_string_lossy().into_owned()
        }}"#
                ),
            ),
            ty if !fallible => (rust_value_type(ty)?, call),
            ty => (
                format!("Result<{}, Error>", rust_value_type(ty)?),
                format!("Ok({call})"),
            ),
        })
    }
}

fn rust_value_type(cpp_type: &str) -> anyhow::Result<String> {
    let cpp_type = cpp_type
        .trim()
        .strip_prefix("const ")
        .unwrap_or(cpp_type.trim());
    let rust_type = match cpp_type {
        "bool" => String::from("bool"),
        "int" => String::from("i32"),
        "unsigned int" => String::from("u32"),
        "short" => String::from("i16"),
        "unsigned short" => String::from("u16"),
        "char" => String::from("c_char"),
        "double" => String::from("f64"),
        "float" => String::from("f32"),
        ty if ty.starts_with("TThostFtdc") || ty.starts_with("THOST_") => {
            format!("sys::{ty}")
        }
        _ => anyhow::bail!("unsupported Rust API value type: {cpp_type}"),
    };
    Ok(rust_type)
}

impl Param {
    fn is_string_array(&self) -> bool {
        matches!(
            self.cpp_type
                .split_whitespace()
                .collect::<String>()
                .as_str(),
            "char**" | "char*[]"
        )
    }

    fn is_string(&self) -> bool {
        matches!(
            self.cpp_type
                .split_whitespace()
                .collect::<String>()
                .as_str(),
            "char*" | "constchar*" | "char**" | "char*[]"
        )
    }

    fn rust_codegen(&self) -> anyhow::Result<(String, String, String)> {
        let name = self.name.to_snake_case();
        let cpp_type = self.cpp_type.split_whitespace().collect::<String>();
        let mut setup = String::new();
        let (rust_type, arg) = match cpp_type.as_str() {
            "char*" => {
                setup = format!(
                    r#"
        let mut {name}_buffer = c_string({name})?;
"#
                );
                (String::from("&str"), format!("{name}_buffer.as_mut_ptr()"))
            }
            "constchar*" => {
                setup = format!(
                    r#"
        let {name}_buffer = CString::new({name})?;
"#
                );
                (String::from("&str"), format!("{name}_buffer.as_ptr()"))
            }
            ty if ty.ends_with('*') => {
                let pointee = &ty[..ty.len() - 1];
                let (reference, pointee) = match pointee.strip_prefix("const") {
                    Some(pointee) => ("&", pointee),
                    None => ("&mut ", pointee),
                };
                anyhow::ensure!(
                    pointee.starts_with("CThostFtdc") && pointee.ends_with("Field"),
                    "unsupported API pointer: {}",
                    self.cpp_type
                );
                (format!("{reference}sys::{pointee}"), name.clone())
            }
            _ => (rust_value_type(&self.cpp_type)?, name.clone()),
        };
        Ok((format!("{name}: {rust_type}"), setup, arg))
    }
}
