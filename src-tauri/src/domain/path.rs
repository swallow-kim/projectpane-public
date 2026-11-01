use super::{Fault, FaultCode};
use std::path::{Path, PathBuf};

/// A Unicode absolute path validated without filesystem access or normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbsolutePath(String);

impl AbsolutePath {
    pub fn new(path: String) -> Result<Self, Fault> {
        if path.len() > 32_768 || path.contains('\0') || !Path::new(&path).is_absolute() {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn into_boxed_path(self) -> Box<Path> {
        PathBuf::from(self.0).into_boxed_path()
    }
}
