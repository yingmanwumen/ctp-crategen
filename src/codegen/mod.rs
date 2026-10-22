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
    codegen::{
        model::{Class, ErrorCode, RequestStruct},
        parser::Parser,
    },
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
    crate_name: String,
    error_xml: bool,
}

impl Codegen {
    pub fn new(output_dir: PathBuf, crate_name: String, error_xml: bool) -> Self {
        Self {
            output_dir,
            crate_name,
            error_xml,
        }
    }

    pub fn generate(&self, sdk: &Sdk) -> anyhow::Result<()> {
        let errors = if self.error_xml {
            sdk.get_error_xml()?
                .map(|text| parser::parse_error_codes(&text))
                .transpose()?
        } else {
            None
        };
        self.copy_sdk(sdk)?;
        let (classes, requests) = self.parse_classes(sdk)?;
        let (hpp, cpp) = self.cpp_bridge_codegen(&classes)?;

        Self::write_file(self.output_dir.join("vendor").join(BRIDGE_HPP_NAME), &hpp)?;
        Self::write_file(self.output_dir.join("vendor").join(BRIDGE_CPP_NAME), &cpp)?;

        let rust_binding = self.rust_bindgen(sdk, &hpp)?;
        let rust_sys = format!(
            r#"#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
{}"#,
            rust_binding
        );
        Self::write_file(self.output_dir.join("src").join("sys.rs"), &rust_sys)?;
        Self::write_file(self.output_dir.join("build.rs"), &self.build_rs_codegen())?;
        Self::write_file(
            self.output_dir.join("Cargo.toml"),
            &self.cargo_toml_codegen(),
        )?;

        let rust_lib = self.rust_lib_codegen(&classes)?;
        Self::write_file(
            self.output_dir.join("src").join("error.rs"),
            &self.error_codegen(errors.as_deref()),
        )?;
        Self::write_file(self.output_dir.join("src").join("lib.rs"), &rust_lib)?;
        let mut builders = String::from(REQUEST_BUILDERS_SUPPORT);
        for request in &requests {
            builders += &request.rust_codegen();
        }
        Self::write_file(self.output_dir.join("src").join("requests.rs"), &builders)?;

        Ok(())
    }

    fn build_rs_codegen(&self) -> String {
        r#"use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let vendor = root.join("vendor");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let bridge = out.join("libctp_bridge.so");

    // Also detect added or removed SDK libraries and headers.
    println!("cargo:rerun-if-changed={}", vendor.display());
    println!("cargo:rerun-if-env-changed=CXX");

    // Any `cargo:rerun-if-*` directive disables Cargo's default change
    // detection, so watch the generated crate's own files explicitly.
    println!("cargo:rerun-if-changed={}", root.join("build.rs").display());
    println!("cargo:rerun-if-changed={}", root.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", root.join("src").join("sys.rs").display());
    println!("cargo:rerun-if-changed={}", root.join("src").join("lib.rs").display());
    if root.join("src").join("error.rs").exists() {
        println!("cargo:rerun-if-changed={}", root.join("src").join("error.rs").display());
    }

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
        .arg(&bridge);

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
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
build = "build.rs"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
thiserror = "2"
"#,
            self.crate_name
        )
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

    fn parse_classes(
        &self,
        sdk: &Sdk,
    ) -> anyhow::Result<(HashMap<&str, Class>, Vec<RequestStruct>)> {
        let clang = Clang::new().map_err(|e| anyhow::anyhow!(e))?;
        let index = Index::new(&clang, false, false);
        let parser = Parser::new(sdk, &index)?;

        let classes = CLASSES
            .iter()
            .map(|&class_name| parser.parse(class_name).map(|res| (class_name, res)))
            .collect::<anyhow::Result<HashMap<_, _>>>()?;
        let requests = parser.request_structs(&classes)?;
        Ok((classes, requests))
    }

    /// Generate .hpp & .cpp
    fn cpp_bridge_codegen(
        &self,
        classes: &HashMap<&str, Class>,
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
            let (class_hpp, class_cpp) = class.cpp_codegen();
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
            .derive_default(true)
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

    fn error_codegen(&self, errors: Option<&[ErrorCode]>) -> String {
        let mut rust = String::from(ERROR_SUPPORT);
        let Some(errors) = errors.filter(|errors| !errors.is_empty()) else {
            return rust;
        };
        let (mut variants, mut from_codes, mut messages) =
            (String::new(), String::new(), String::new());
        for error in errors {
            let (variant, from_code, message) = error.rust_codegen();
            variants += &variant;
            from_codes += &from_code;
            messages += &message;
        }
        rust += &format!(
            r#"
/// CTP business response codes from error.xml, not immediate API request return codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BusinessErrorCode {{
{variants}}}

impl TryFrom<i32> for BusinessErrorCode {{
    type Error = i32;

    fn try_from(code: i32) -> Result<Self, Self::Error> {{
        match code {{
{from_codes}            code => Err(code),
        }}
    }}
}}

impl BusinessErrorCode {{
    /// Returns the SDK's generic message, not the response's potentially more specific ErrorMsg.
    pub const fn message(self) -> &'static str {{
        match self {{
{messages}        }}
    }}
}}
"#
        );
        rust
    }

    fn rust_lib_codegen(&self, classes: &HashMap<&str, Class>) -> anyhow::Result<String> {
        let mut rust = String::from(
            r#"pub mod sys;
pub mod error;
pub mod requests;
pub use error::{Error, RequestError};
pub use requests::*;

use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::PathBuf,
    ptr::NonNull,
};

fn request_result(code: i32) -> Result<(), RequestError> {
    if code == 0 {
        Ok(())
    } else {
        Err(RequestError { code })
    }
}

fn c_string(value: &str) -> Result<Vec<c_char>, Error> {
    Ok(CString::new(value)?
        .into_bytes_with_nul()
        .into_iter()
        .map(|byte| byte as c_char)
        .collect())
}

"#,
        );
        for class_name in CLASSES.into_iter().filter(|name| name.ends_with("Spi")) {
            let class = classes
                .get(class_name)
                .ok_or_else(|| anyhow::anyhow!("missing SPI class: {class_name}"))?;
            rust += &class.rust_spi_codegen()?;
        }
        for class_name in CLASSES {
            if class_name.ends_with("Api") {
                let class = classes
                    .get(class_name)
                    .ok_or_else(|| anyhow::anyhow!("missing API class: {class_name}"))?;
                rust += &class.rust_codegen()?;
            }
        }
        Ok(rust)
    }
}

const ERROR_SUPPORT: &str = r#"//! Errors from the Rust wrapper and immediate SDK request returns.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("CTP request returned {code}")]
pub struct RequestError {
    pub code: i32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid C string: {0}")]
    Nul(#[from] std::ffi::NulError),
    #[error("failed to create {0}")]
    CreateFailed(&'static str),
    #[error("instrument count exceeds c_int")]
    TooManyInstruments,
    #[error(transparent)]
    Request(#[from] RequestError),
}
"#;

const REQUEST_BUILDERS_SUPPORT: &str = r#"//! Builders for bindgen request types. Unset fields retain their zero defaults.
//! Text setters write UTF-8 bytes; use the public raw fields for other encodings.

use crate::sys;
use std::ffi::c_char;

#[derive(Debug, thiserror::Error)]
pub enum FieldError {
    #[error("{field} contains an interior NUL byte")]
    InteriorNul { field: &'static str },
    #[error("{field} is {length} bytes long, but its text capacity is {capacity} bytes")]
    TooLong { field: &'static str, length: usize, capacity: usize },
}

fn write_text<const N: usize>(target: &mut [c_char; N], value: &str, field: &'static str) -> Result<(), FieldError> {
    if value.as_bytes().contains(&0) {
        return Err(FieldError::InteriorNul { field });
    }
    if N == 0 || value.len() >= N {
        return Err(FieldError::TooLong { field, length: value.len(), capacity: N.saturating_sub(1) });
    }
    target.fill(0);
    for (target, byte) in target.iter_mut().zip(value.bytes()) {
        *target = byte as c_char;
    }
    Ok(())
}

"#;
