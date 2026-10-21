//! Code generation pipeline for converting validated CTP SDK data into crates.

mod model;
mod parser;

use std::collections::HashMap;

use bindgen::Bindings;
use clang::{Clang, Index};

use crate::{
    codegen::{model::Class, parser::Parser},
    sdk::{CLASSES, Sdk},
};

/// 1. Generate C bridge code
/// 2. Compile ctp_c.so
/// 3. Rust bindgen
/// 4. Combine them into a crate/module
pub struct Codegen {}

impl Codegen {
    pub fn generate(&self, sdk: &Sdk) -> anyhow::Result<()> {
        let classes = self.parse_classes(sdk)?;
        self.cpp_bridge_gen(classes)?;

        let _rust_binding = self.rust_bindgen(sdk)?;
        Ok(())
    }

    fn parse_classes(&self, sdk: &Sdk) -> anyhow::Result<HashMap<&str, Class>> {
        let clang = Clang::new().map_err(|e| anyhow::anyhow!(e))?;
        let index = Index::new(&clang, false, false);
        let parser = Parser::new(sdk, &index)?;

        CLASSES
            .iter()
            .map(|&class_name| parser.parse(class_name).map(|res| (class_name, res)))
            .collect::<anyhow::Result<HashMap<_, _>>>()
    }

    fn cpp_bridge_gen(&self, classes: HashMap<&str, Class>) -> anyhow::Result<()> {
        Ok(())
    }

    fn rust_bindgen(&self, sdk: &Sdk) -> anyhow::Result<Bindings> {
        Ok(bindgen::Builder::default()
            .header_contents(
                "ThostFtdcUserApiStruct.h",
                sdk.get_header("ThostFtdcUserApiStruct.h").unwrap(),
            )
            .header_contents(
                "ThostFtdcUserApiDataType.h",
                sdk.get_header("ThostFtdcUserApiDataType.h").unwrap(),
            )
            .generate()?)
    }
}
