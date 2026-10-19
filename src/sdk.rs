use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use crate::misc::dir::DirBuf;
use encoding_rs::{GBK, UTF_8};
use thiserror::Error;

const HEADERS: [&str; 4] = [
    "ThostFtdcTraderApi.h",
    "ThostFtdcMdApi.h",
    "ThostFtdcUserApiStruct.h",
    "ThostFtdcUserApiDataType.h",
];

const LIBS: [&str; 2] = ["thosttraderapi_se.so", "thostmduserapi_se.so"];

#[derive(Debug, Clone)]
/// An SDK located at a validated directory.
pub struct Sdk {
    dir: DirBuf,
    /// Header file name to UTF-8 contents.
    headers: HashMap<&'static str, String>,
}

#[derive(Debug, Error)]
/// Errors that can occur while verifying an SDK directory.
pub enum VerifyError {
    /// The SDK directory could not be read.
    #[error("failed to read SDK directory")]
    Io(#[from] std::io::Error),
    /// One or more required SDK files https://crates.io/search?q=ctp-crategenare missing.
    #[error("missing SDK files: {0:?}")]
    Missing(Vec<&'static str>),
    #[error("cannot detect encoding of file: {0:?}")]
    Encoding(PathBuf),
}

impl Sdk {
    /// Creates an SDK from a directory path.
    pub fn new(dir: &str) -> anyhow::Result<Self> {
        let dir = DirBuf::try_from(dir)?;
        Self::verify(&dir)?;

        // Read all required headers once so later code generation does not need
        // to perform additional filesystem or encoding work.
        let headers = HEADERS
            .iter()
            .copied()
            .map(|header| Self::read_text(&dir.as_ref().join(header)).map(|text| (header, text)))
            .collect::<Result<HashMap<_, _>, VerifyError>>()?;
        Ok(Self { dir, headers })
    }

    /// Returns the decoded contents of a required header by its file name.
    ///
    /// Returns `None` when `header` is not one of the required SDK headers.
    pub fn get_header(&self, header: &'static str) -> Option<&String> {
        self.headers.get(header)
    }

    /// Verifies that every required header and shared library exists in the SDK
    /// root directory before any header is read.
    fn verify(dir: &DirBuf) -> Result<(), VerifyError> {
        let entries: HashSet<OsString> = fs::read_dir(dir.as_ref())?
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

    fn read_text(file: &Path) -> Result<String, VerifyError> {
        let bytes = fs::read(file)?;

        // UTF-8 is preferred because it is the native representation of Rust
        // strings. CTP SDKs distributed in Chinese environments may instead
        // contain GBK-encoded headers, so GBK is used as a fallback.
        if let Some(text) = UTF_8.decode_without_bom_handling_and_without_replacement(&bytes) {
            return Ok(text.into_owned());
        }

        if let Some(text) = GBK.decode_without_bom_handling_and_without_replacement(&bytes) {
            return Ok(text.into_owned());
        }

        Err(VerifyError::Encoding(file.to_owned()))
    }
}

impl AsRef<Path> for Sdk {
    fn as_ref(&self) -> &Path {
        self.dir.as_ref()
    }
}
