use std::path::{Path, PathBuf};

use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct DirBuf(PathBuf);

impl DirBuf {
    fn parse_directory(value: &str) -> anyhow::Result<PathBuf> {
        let path = PathBuf::from(value);
        if path.is_dir() {
            Ok(path.canonicalize()?)
        } else {
            Err(anyhow!("{value:?} is not a directory"))
        }
    }
}

impl AsRef<Path> for DirBuf {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<&str> for DirBuf {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(Self::parse_directory(value)?))
    }
}
