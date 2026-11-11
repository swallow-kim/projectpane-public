//! Global folder shortcuts. Paths are syntax checked, never probed on load/save:
//! offline shares must not block the UI or disappear from the saved list.
use super::{ProjectError, ProjectService, api::fail};
use crate::{domain::projects::normalize_absolute_path, persistence::projects::Transaction};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Favorite {
    pub name: String,
    pub path: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Favorites {
    pub schema_version: u32,
    pub revision: u64,
    pub folders: Vec<Favorite>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveFavorites {
    pub expected_revision: u64,
    pub folders: Vec<Favorite>,
}
const MAX_REVISION: u64 = 9_007_199_254_740_991;
const MAX_BYTES: usize = 256 * 1024;

fn normalize(folders: &mut [Favorite]) -> io::Result<()> {
    if folders.len() > 128 {
        return Err(fail("payload_limit"));
    }
    let mut paths = HashSet::new();
    for folder in folders {
        if folder.name.trim().is_empty()
            || folder.name.chars().count() > 128
            || folder.name.chars().any(char::is_control)
        {
            return Err(fail("invalid_document"));
        }
        folder.path = normalize_absolute_path(&folder.path).map_err(|_| fail("invalid_path"))?;
        if !paths.insert(folder.path.to_lowercase()) {
            return Err(fail("duplicate_path"));
        }
    }
    Ok(())
}
fn parse(bytes: &[u8]) -> io::Result<Favorites> {
    if bytes.len() > MAX_BYTES {
        return Err(fail("payload_limit"));
    }
    let mut value: Favorites =
        serde_json::from_slice(bytes).map_err(|_| fail("invalid_document"))?;
    if value.schema_version != 1 {
        return Err(fail("unsupported_schema"));
    }
    if value.revision > MAX_REVISION {
        return Err(fail("invalid_document"));
    }
    normalize(&mut value.folders)?;
    Ok(value)
}
fn read(tx: &Transaction<'_>) -> io::Result<Favorites> {
    match tx.read("favorites.json")? {
        Some(bytes) => parse(&bytes),
        None => {
            // Missing primary with a backup is recovery evidence, not an empty list.
            if tx.read_backup("favorites.json")?.is_some() {
                return Err(fail("invalid_document"));
            }
            Ok(Favorites {
                schema_version: 1,
                revision: 0,
                folders: vec![],
            })
        }
    }
}
impl ProjectService {
    pub fn list_favorites(&self) -> Result<Favorites, ProjectError> {
        self.transaction(read).map_err(Into::into)
    }
    pub fn save_favorites(&self, request: SaveFavorites) -> Result<Favorites, ProjectError> {
        self.transaction(|tx| {
            let current = read(tx)?;
            if current.revision != request.expected_revision || current.revision == MAX_REVISION {
                return Err(fail("revision_conflict"));
            }
            let mut value = Favorites {
                schema_version: 1,
                revision: current.revision + 1,
                folders: request.folders,
            };
            normalize(&mut value.folders)?;
            let bytes = serde_json::to_vec(&value).map_err(|_| fail("invalid_document"))?;
            tx.write("favorites.json", &bytes, |bytes| parse(bytes).map(|_| ()))?;
            Ok(value)
        })
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::projects::ProjectStore;

    #[test]
    fn favorites_persist_preserve_offline_paths_and_reject_lost_updates() {
        let _guard = super::super::TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let service = ProjectService::new(ProjectStore::new(temp.path(), None).unwrap());
        assert_eq!(service.list_favorites().unwrap().revision, 0);
        let folders = vec![Favorite {
            name: "Offline".into(),
            path: r"\\missing-host\share\work".into(),
        }];
        let saved = service
            .save_favorites(SaveFavorites {
                expected_revision: 0,
                folders: folders.clone(),
            })
            .unwrap();
        let reopened = ProjectService::new(ProjectStore::new(temp.path(), None).unwrap());
        assert_eq!(reopened.list_favorites().unwrap(), saved);
        assert_eq!(
            reopened
                .save_favorites(SaveFavorites {
                    expected_revision: 0,
                    folders: vec![]
                })
                .unwrap_err()
                .code,
            "revision_conflict"
        );
        let duplicate = vec![
            Favorite {
                name: "A".into(),
                path: r"C:\Work".into(),
            },
            Favorite {
                name: "B".into(),
                path: "c:/work/./".into(),
            },
        ];
        assert_eq!(
            service
                .save_favorites(SaveFavorites {
                    expected_revision: 1,
                    folders: duplicate
                })
                .unwrap_err()
                .code,
            "duplicate_path"
        );
        assert_eq!(reopened.list_favorites().unwrap(), saved);
        // Corruption is never silently reset or overwritten, including by save.
        let path = temp.path().join("foundation/projects-v1/favorites.json");
        std::fs::write(&path, b"invalid").unwrap();
        assert!(service.list_favorites().is_err());
        assert!(
            service
                .save_favorites(SaveFavorites {
                    expected_revision: 1,
                    folders
                })
                .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), b"invalid");
    }
    #[test]
    fn favorite_contract_is_bounded_and_does_not_accept_device_paths() {
        let mut folders = vec![Favorite {
            name: "folder".into(),
            path: r"\\?\C:\Work".into(),
        }];
        assert!(normalize(&mut folders).is_err());
        folders[0].path = "C:/Work/../Next/".into();
        normalize(&mut folders).unwrap();
        assert_eq!(folders[0].path, r"C:\Next");
        folders[0].name = "\n".into();
        assert!(normalize(&mut folders).is_err());
        assert!(parse(br#"{"schemaVersion":2,"revision":0,"folders":[]}"#).is_err());
    }
}
