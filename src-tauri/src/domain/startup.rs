use super::{AbsolutePath, MAX_FRAME};
use serde::Serialize;
use std::ffi::OsString;

#[derive(Clone, Serialize)]
pub struct StartupConfig {
    pub folders: Vec<String>,
    pub error: Option<String>,
}

impl StartupConfig {
    pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut args = args.into_iter();
        let mut folders = Vec::new();
        while let Some(flag) = args.next() {
            if flag != "--folder" || folders.len() == 4 {
                return Self::invalid();
            }
            let Some(value) = args.next() else {
                return Self::invalid();
            };
            let Ok(path) = value.into_string() else {
                return Self::invalid();
            };
            let Ok(path) = AbsolutePath::new(path) else {
                return Self::invalid();
            };
            folders.push(path.into_string());
        }
        let config = Self {
            folders,
            error: None,
        };
        // JSON escaping can expand otherwise valid paths beyond the IPC boundary.
        match serde_json::to_vec(&config) {
            Ok(payload) if payload.len() < MAX_FRAME => config,
            Ok(_) | Err(_) => Self::invalid(),
        }
    }

    fn invalid() -> Self {
        Self {
            folders: Vec::new(),
            error: Some("invalid_startup_args".to_owned()),
        }
    }
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod tests;
