//! Code generation pipeline for converting validated CTP SDK data into crates.

mod model;
mod parser;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use bindgen::Bindings;
use clang::{Clang, Index};

use crate::{
    codegen::{model::Class, parser::Parser},
    sdk::{CLASSES, HEADERS, Sdk},
};

const BRIDGE_CPP_NAME: &str = "ctp_brdige.cpp";
const BRIDGE_HPP_NAME: &str = "ctp_brdige.hpp";

/// 1. Generate C bridge code
/// 2. Compile ctp_c.so
/// 3. Rust bindgen
/// 4. Combine them into a crate/module
pub struct Codegen {
    output_dir: PathBuf,
}

impl Codegen {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    pub fn generate(&self, sdk: &Sdk) -> anyhow::Result<()> {
        self.copy_sdk(sdk)?;
        let classes = self.parse_classes(sdk)?;
        let (hpp, cpp) = self.cpp_bridge_codegen(classes)?;

        Self::write_file(self.output_dir.join("vendor").join(BRIDGE_HPP_NAME), &hpp)?;
        Self::write_file(self.output_dir.join("vendor").join(BRIDGE_CPP_NAME), &cpp)?;

        let rust_binding = self.rust_bindgen(sdk, &hpp)?;
        Self::write_file(
            self.output_dir.join("src").join("sys.rs"),
            &rust_binding.to_string(),
        )?;
        Self::write_file(self.output_dir.join("build.rs"), &self.build_rs_codegen())?;
        Self::write_file(self.output_dir.join("Cargo.toml"), &self.cargo_toml_codegen())?;
        Ok(())
    }

    fn build_rs_codegen(&self) -> String {
        r#"use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let vendor = root.join("vendor");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    // Also detect added or removed SDK libraries and headers.
    println!("cargo:rerun-if-changed={}", vendor.display());
    println!("cargo:rerun-if-env-changed=CXX");

    let mut libraries = fs::read_dir(&vendor)
        .expect("failed to read vendor directory")
        .map(|entry| entry.expect("failed to read vendor entry").path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|ext| ext == "so")
        })
        .collect::<Vec<_>>();
    libraries.sort();

    let mut compiler = Command::new(env::var_os("CXX").unwrap_or_else(|| "c++".into()));
    compiler
        .args(["-std=c++11", "-fPIC", "-shared"])
        .arg(vendor.join("ctp_brdige.cpp"))
        .arg("-I")
        .arg(&vendor)
        .arg("-L")
        .arg(&vendor);

    for library in libraries {
        let filename = library.file_name().unwrap().to_str().unwrap();
        println!("cargo:rerun-if-changed={}", library.display());
        compiler.arg(format!("-l:{filename}"));
    }

    compiler
        .arg("-Wl,-z,defs")
        .arg("-Wl,-soname,libctp_bridge.so")
        .arg(format!("-Wl,-rpath,{}", vendor.display()))
        .arg("-o")
        .arg(out.join("libctp_bridge.so"));

    let status = compiler.status().expect("failed to start C++ compiler");
    assert!(status.success(), "bridge compilation failed: {compiler:?}");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=dylib=ctp_bridge");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", out.display());
}
"#
        .to_owned()
    }

    fn cargo_toml_codegen(&self) -> String {
        r#"[package]
name = "ctp-sys"
version = "0.1.0"
edition = "2024"
build = "build.rs"
publish = false

[lib]
path = "src/lib.rs"
"#
        .to_owned()
    }

    fn copy_sdk(&self, sdk: &Sdk) -> anyhow::Result<()> {
        let vendor_dir = self.output_dir.join("vendor");
        std::fs::create_dir_all(&vendor_dir)?;

        for header in HEADERS {
            std::fs::copy(sdk.as_ref().join(header), vendor_dir.join(header))?;
        }

        for entry in std::fs::read_dir(sdk.as_ref())? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "so") {
                std::fs::copy(&path, vendor_dir.join(entry.file_name()))?;
            }
        }

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

    /// Generate .hpp & .cpp
    fn cpp_bridge_codegen(
        &self,
        classes: HashMap<&str, Class>,
    ) -> anyhow::Result<(String, String)> {
        let mut hpp = String::from(
            r#"
#pragma once

#include <stdbool.h>

#include "ThostFtdcUserApiStruct.h"
#include "ThostFtdcUserApiDataType.h"

#ifdef __cplusplus
extern "C" {
#endif
"#,
        );
        let mut cpp = format!(
            r#"
#include "{BRIDGE_HPP_NAME}"
#include "ThostFtdcTraderApi.h"
#include "ThostFtdcMdApi.h"

#include <new>
"#
        );
        for class in classes.values() {
            let (class_hpp, class_cpp) = class.codegen();
            hpp += &class_hpp;
            cpp += &class_cpp;
        }
        hpp += r#"
#ifdef __cplusplus
}
#endif
"#;
        Ok((hpp, cpp))
    }

    fn rust_bindgen(&self, sdk: &Sdk, wrapper_hpp: &str) -> anyhow::Result<Bindings> {
        Ok(bindgen::Builder::default()
            .clang_args(["-x", "c++"])
            .header_contents(
                "ThostFtdcUserApiStruct.h",
                sdk.get_header("ThostFtdcUserApiStruct.h").unwrap(),
            )
            .header_contents(
                "ThostFtdcUserApiDataType.h",
                sdk.get_header("ThostFtdcUserApiDataType.h").unwrap(),
            )
            .header_contents(BRIDGE_HPP_NAME, wrapper_hpp)
            .generate()?)
    }

    fn write_file(path: impl AsRef<Path>, content: &str) -> std::io::Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)
    }
}
