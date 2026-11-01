use crate::domain::{Fault, FaultCode};
use serde::{Deserialize, Serialize};

pub const MAX_WATCH_PATHS: usize = 4;
#[cfg(windows)]
pub const WATCH_FRAME: u64 = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WatchRequest {
    Replace { generation: u32, paths: Vec<String> },
    Poll { generation: u32 },
}

impl WatchRequest {
    pub fn generation(&self) -> u32 {
        match self {
            Self::Replace { generation, .. } | Self::Poll { generation } => *generation,
        }
    }

    pub fn validate(&self) -> Result<(), Fault> {
        if self.generation() == 0 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        if let Self::Replace { paths, .. } = self {
            if paths.len() > MAX_WATCH_PATHS {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            for path in paths {
                let bytes = path.as_bytes();
                if !(3..=32_768).contains(&bytes.len())
                    || !bytes[0].is_ascii_alphabetic()
                    || &bytes[1..3] != b":\\"
                    || path.contains(['\0', '/'])
                    || (path.len() > 3
                        && path[3..].split('\\').any(|part| {
                            part.is_empty() || matches!(part, "." | "..") || part.contains(':')
                        }))
                {
                    return Err(Fault::new(FaultCode::InvalidPath));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WatchReply {
    Configured {
        generation: u32,
        watched: u8,
        unavailable: u8,
    },
    Polled {
        generation: u32,
        changed: u8,
        unavailable: u8,
    },
}

impl WatchReply {
    pub fn validate(&self, request: &WatchRequest, slots: usize) -> Result<(), Fault> {
        request.validate()?;
        let valid = match (self, request) {
            (
                Self::Configured {
                    generation,
                    watched,
                    unavailable,
                },
                WatchRequest::Replace { paths, .. },
            ) => {
                let unique: std::collections::HashSet<_> = paths
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| unavailable & (1 << index) == 0)
                    .map(|(_, path)| path)
                    .collect();
                let shared = paths.iter().enumerate().all(|(index, path)| {
                    paths.iter().enumerate().all(|(other, value)| {
                        path != value
                            || (unavailable & (1 << index) != 0)
                                == (unavailable & (1 << other) != 0)
                    })
                });
                *generation == request.generation()
                    && usize::from(*watched) == unique.len()
                    && u16::from(*unavailable) < (1_u16 << paths.len())
                    && shared
            }
            (
                Self::Polled {
                    generation,
                    changed,
                    unavailable,
                },
                WatchRequest::Poll { .. },
            ) => {
                *generation == request.generation()
                    && slots <= MAX_WATCH_PATHS
                    && u16::from(*changed) < (1_u16 << slots)
                    && u16::from(*unavailable) < (1_u16 << slots)
                    && changed & unavailable == 0
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(Fault::new(FaultCode::ProtocolError))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unc_aliases_relative_and_oversized_subscriptions() {
        for path in [
            "",
            "relative",
            "C:leaf",
            "\\\\host\\share",
            "\\\\?\\C:\\root",
            "C:\\..\\root",
            "C:\\root\\",
            "C:\\a:b",
            "C:\\a\0b",
        ] {
            assert!(
                WatchRequest::Replace {
                    generation: 1,
                    paths: vec![path.into()]
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            WatchRequest::Replace {
                generation: 1,
                paths: vec!["C:\\".into(); 5]
            }
            .validate()
            .is_err()
        );
        assert!(WatchRequest::Poll { generation: 0 }.validate().is_err());
        WatchRequest::Replace {
            generation: 1,
            paths: vec!["C:\\".into(), "D:\\한글".into()],
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn checks_unique_handle_count_reply_kind_generation_and_slot_mask() {
        let request = WatchRequest::Replace {
            generation: 3,
            paths: vec!["C:\\a".into(); 2],
        };
        WatchReply::Configured {
            generation: 3,
            watched: 1,
            unavailable: 0,
        }
        .validate(&request, 0)
        .unwrap();
        assert!(
            WatchReply::Configured {
                generation: 3,
                watched: 2,
                unavailable: 0,
            }
            .validate(&request, 0)
            .is_err()
        );
        let poll = WatchRequest::Poll { generation: 3 };
        WatchReply::Polled {
            generation: 3,
            changed: 3,
            unavailable: 0,
        }
        .validate(&poll, 2)
        .unwrap();
        assert!(
            WatchReply::Polled {
                generation: 2,
                changed: 0,
                unavailable: 0,
            }
            .validate(&poll, 2)
            .is_err()
        );
        assert!(
            WatchReply::Polled {
                generation: 3,
                changed: 4,
                unavailable: 0,
            }
            .validate(&poll, 2)
            .is_err()
        );
        assert!(
            serde_json::from_str::<WatchRequest>(
                r#"{"kind":"poll","generation":1,"path":"C:\\a"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn unavailable_masks_preserve_shared_path_consistency_and_exclude_changes() {
        let request = WatchRequest::Replace {
            generation: 1,
            paths: vec![
                "C:\\missing".into(),
                "C:\\good".into(),
                "C:\\missing".into(),
            ],
        };
        WatchReply::Configured {
            generation: 1,
            watched: 1,
            unavailable: 5,
        }
        .validate(&request, 0)
        .unwrap();
        for unavailable in [1, 8] {
            assert!(
                WatchReply::Configured {
                    generation: 1,
                    watched: 2,
                    unavailable
                }
                .validate(&request, 0)
                .is_err()
            );
        }
        assert!(
            WatchReply::Polled {
                generation: 1,
                changed: 1,
                unavailable: 1
            }
            .validate(&WatchRequest::Poll { generation: 1 }, 2)
            .is_err()
        );
        WatchReply::Polled {
            generation: 1,
            changed: 2,
            unavailable: 1,
        }
        .validate(&WatchRequest::Poll { generation: 1 }, 2)
        .unwrap();
    }
}
