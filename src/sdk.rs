use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;

use crate::misc::dir::DirBuf;
use thiserror::Error;

#[derive(Debug, Clone)]
/// An SDK located at a validated directory.
pub struct Sdk {
    dir: DirBuf,
}

const HEADERS: [&str; 4] = [
    "ThostFtdcTraderApi.h",
    "ThostFtdcMdApi.h",
    "ThostFtdcUserApiStruct.h",
    "ThostFtdcUserApiDataType.h",
];

const LIBS: [&str; 2] = ["thosttraderapi_se.so", "thostmduserapi_se.so"];

#[derive(Debug, Error)]
/// Errors that can occur while verifying an SDK directory.
pub enum VerifyError {
    /// The SDK directory could not be read.
    #[error("failed to read SDK directory")]
    Io(#[from] std::io::Error),
    /// One or more required SDK files https://crates.io/search?q=ctp-crategenare missing.
    #[error("missing SDK files: {0:?}")]
    Missing(Vec<&'static str>),
}

impl Sdk {
    /// Creates an SDK from a directory path.
    pub fn new(dir: &str) -> anyhow::Result<Self> {
        let sdk = Self {
            dir: DirBuf::try_from(dir)?,
        };
        Ok(sdk.verify().map(|_| sdk)?)
    }

    /// Verifies the required headers and libraries in the SDK root directory.
    pub fn verify(&self) -> Result<(), VerifyError> {
        let entries: HashSet<OsString> = fs::read_dir(self.dir.as_ref())?
            .map(|e| e.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;

        let missing: Vec<&'static str> = HEADERS
            .into_iter()
            .chain(LIBS)
            .filter(|name| !entries.contains(OsStr::new(name)))
            .collect();

        if !missing.is_empty() {
            return Err(VerifyError::Missing(missing));
        }

        Ok(())
    }
}
