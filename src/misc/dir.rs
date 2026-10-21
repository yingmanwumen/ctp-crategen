use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct DirBuf(PathBuf);

impl DirBuf {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        Self::try_from(path)
    }

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

impl Deref for DirBuf {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DirBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TryFrom<&str> for DirBuf {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(Self::parse_directory(value)?))
    }
}
