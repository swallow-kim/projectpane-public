//! Primary-instance-only window state and bounded, payload-free diagnostics.
mod geometry;
mod log_files;
mod logger;
pub mod operations;
mod paths;
pub mod projects;
pub mod recovery;
mod state;

pub use geometry::{WindowBounds, WorkArea, clamp_window};
pub use logger::{BoundedLogger, EventKind, LogError, LogEvent};
pub use paths::resolve_data_root;
pub use state::{StateError, WindowStateStore};
