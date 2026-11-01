use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultCode {
    InvalidRequest,
    InvalidPath,
    InvalidState,
    NotFound,
    AccessDenied,
    IoError,
    ResourceLimit,
    PayloadLimit,
    ProtocolError,
    WorkerUnavailable,
    Busy,
    DestinationExists,
    Cancelled,
    Timeout,
    StaleRequest,
}

impl FaultCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidPath => "invalid_path",
            Self::InvalidState => "invalid_state",
            Self::NotFound => "not_found",
            Self::AccessDenied => "access_denied",
            Self::IoError => "io_error",
            Self::ResourceLimit => "resource_limit",
            Self::PayloadLimit => "payload_limit",
            Self::ProtocolError => "protocol_error",
            Self::WorkerUnavailable => "worker_unavailable",
            Self::Busy => "busy",
            Self::DestinationExists => "destination_exists",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::StaleRequest => "stale_request",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub code: FaultCode,
}

impl Fault {
    pub const fn new(code: FaultCode) -> Self {
        Self { code }
    }
}

impl From<io::Error> for Fault {
    fn from(error: io::Error) -> Self {
        Self::new(match error.kind() {
            io::ErrorKind::NotFound => FaultCode::NotFound,
            io::ErrorKind::PermissionDenied => FaultCode::AccessDenied,
            _ => FaultCode::IoError,
        })
    }
}
