mod archive;
mod archive_pipe;
mod budget;
mod copy_as;
#[cfg(test)]
mod copy_as_boundary_tests;
#[cfg(test)]
mod copy_as_tests;
mod copy_tree;
mod copy_tree_sink;
mod execute;
mod folder_recycle;
mod guarded_io;
#[cfg(test)]
pub(crate) mod guarded_move;
mod identity;
mod lane;
mod message_pump;
mod move_tree;
mod move_tree_sink;
mod preflight;
mod prepare;
mod progress;
mod recycle;
mod result;
mod sink;
mod sink_checks;
mod sink_state;
mod skip_existing;
mod wsl_move;
mod zip_directory;

pub use folder_recycle::FolderRecycleSnapshot;
pub use identity::capture_source;
pub use lane::NativeFileOperationLane;
pub use preflight::check_plan;
pub use progress::{NativeProgress, NativeProgressSnapshot};

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod directory_copy_tests;
#[cfg(test)]
mod directory_move_tests;
#[cfg(test)]
mod move_tree_tests;
#[cfg(test)]
mod partial_tests;
#[cfg(test)]
mod recycle_batch_tests;
#[cfg(test)]
mod recycle_tests;
#[cfg(test)]
mod skip_existing_boundary_tests;
#[cfg(test)]
mod skip_existing_budget_tests;
#[cfg(test)]
mod skip_existing_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_boundary_tests;
#[cfg(test)]
mod wsl_tests;
