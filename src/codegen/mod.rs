//! Code generation pipeline for converting validated CTP SDK data into crates.

use crate::sdk::Sdk;

pub struct Codegen {}

impl Codegen {
    pub fn generate(&self, sdk: &Sdk) -> anyhow::Result<()> {
        self.bindgen(sdk)?;
        Ok(())
    }

    fn bindgen(&self, sdk: &Sdk) -> anyhow::Result<()> {
        let user_api_struct = bindgen::Builder::default()
            .header_contents(
                "ThostFtdcUserApiStruct.h",
                sdk.get_header("ThostFtdcUserApiStruct.h").unwrap(),
            )
            .header_contents(
                "ThostFtdcUserApiDataType.h",
                sdk.get_header("ThostFtdcUserApiDataType.h").unwrap(),
            )
            .generate()?;
        user_api_struct.write_to_file("./tmp.rs")?;
        Ok(())
    }
}
