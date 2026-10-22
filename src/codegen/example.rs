use std::collections::HashMap;

use super::{Codegen, model::Class};

impl Codegen {
    pub(super) fn version_example_codegen(&self) -> String {
        include_str!("example/version.rs")
            .replace("__CRATE_NAME__", &self.crate_name.replace('-', "_"))
    }

    pub(super) fn basic_example_codegen(
        &self,
        classes: &HashMap<&str, Class>,
        has_business_errors: bool,
    ) -> anyhow::Result<String> {
        let private_topic = classes
            .get("CThostFtdcTraderApi")
            .and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name == "SubscribePrivateTopic")
            })
            .ok_or_else(|| anyhow::anyhow!("missing SubscribePrivateTopic for basic example"))?;
        let mut topic_args = String::from("sys::THOST_TE_RESUME_TYPE_THOST_TERT_QUICK");
        for param in private_topic.params.iter().skip(1) {
            anyhow::ensure!(
                param.cpp_type.trim() == "int",
                "unsupported SubscribePrivateTopic parameter in basic example: {}",
                param.name
            );
            let default = param
                .default
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing default for {}", param.name))?
                .trim()
                .parse::<i32>()?;
            topic_args += &format!(", {default}");
        }
        let crate_name = self.crate_name.replace('-', "_");
        let business_message = if has_business_errors {
            format!(
                r#"match {crate_name}::error::BusinessErrorCode::try_from(code) {{
        Ok(error) => error.message().to_owned(),
        Err(code) => format!("unknown business error {{code}}"),
    }}"#
            )
        } else {
            String::from("format!(\"business error {code}\")")
        };
        Ok(include_str!("example/basic.rs")
            .replace("__CRATE_NAME__", &crate_name)
            .replace("__PRIVATE_TOPIC_ARGS__", &topic_args)
            .replace("__BUSINESS_MESSAGE__", &business_message))
    }
}
