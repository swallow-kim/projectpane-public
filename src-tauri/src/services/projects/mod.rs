mod api;
mod documents;
mod favorites;
use super::id;
pub use favorites::{Favorite, Favorites, SaveFavorites};
mod loading;
mod mutations;

// All store-using tests share the production process-wide writer. Serialize
// independent fixtures, while keeping each test's deliberate races intact.
#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use crate::persistence::projects::ProjectStore;
pub use api::*;

pub struct ProjectService {
    store: ProjectStore,
}
impl ProjectService {
    pub const fn new(store: ProjectStore) -> Self {
        Self { store }
    }
}

#[cfg(all(test, windows))]
mod tests;
