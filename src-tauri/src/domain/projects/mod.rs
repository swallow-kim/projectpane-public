mod codec;
mod model;
mod path;
mod recovery;
mod validation;

pub use codec::*;
pub use model::*;
pub use path::*;
pub use recovery::*;

pub const MAX_PROJECT_BYTES: usize = 256 * 1024;
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectContractError {
    InvalidDocument,
    UnsupportedSchema,
    PayloadLimit,
    InvalidPath,
}
impl std::fmt::Display for ProjectContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDocument => "invalid_document",
            Self::UnsupportedSchema => "unsupported_schema",
            Self::PayloadLimit => "payload_limit",
            Self::InvalidPath => "invalid_path",
        })
    }
}
impl std::error::Error for ProjectContractError {}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod boundary_tests;
