use super::*;
use crate::domain::{EntryKind, FaultCode, MAX_FRAME};

fn source(path: &str, kind: EntryKind, index: u64) -> OperationSource {
    OperationSource {
        path: path.into(),
        kind,
        volume: 1,
        file_index: index,
    }
}
fn plan(descendants: Vec<OperationSource>) -> OperationPlan {
    OperationPlan {
        project_id: None,
        action: PreparedAction::CopyTree {
            roots: vec![source(r"C:\root", EntryKind::Folder, 1)],
            descendants,
            destination: source(r"D:\target", EntryKind::Folder, 2),
        },
    }
}

#[test]
fn tree_contract_rejects_dangling_ancestors_alias_roots_and_foreign_volumes() {
    for child in [
        source(r"C:\outside\a", EntryKind::File, 3),
        source(r"C:\root\missing\a", EntryKind::File, 3),
        source(r"C:\root\alias", EntryKind::Folder, 1),
        OperationSource {
            volume: 2,
            ..source(r"C:\root\foreign", EntryKind::File, 3)
        },
    ] {
        assert!(plan(vec![child]).validate().is_err());
    }
    let mut overlap = plan(vec![]);
    if let PreparedAction::CopyTree { roots, .. } = &mut overlap.action {
        roots.push(source(r"C:\root\child", EntryKind::Folder, 3));
    }
    assert!(overlap.validate().is_err());
    let alias = source(r"C:\root\a", EntryKind::File, 3);
    assert!(plan(vec![alias.clone(), alias]).validate().is_err());
}

#[test]
fn tree_contract_keeps_hardlink_names_and_full_native_identity_in_durable_intent() {
    let index = u64::MAX;
    let plan = plan(vec![
        source(r"C:\root\a", EntryKind::File, index),
        source(r"C:\root\b", EntryKind::File, index),
    ]);
    let intent = OperationIntent {
        schema_version: 1,
        id: "tree-1".into(),
        plan,
    };
    intent.validate().unwrap();
    let bytes = serde_json::to_vec(&intent).unwrap();
    let loaded: OperationIntent = serde_json::from_slice(&bytes).unwrap();
    loaded.validate().unwrap();
    assert_eq!(loaded, intent);
    assert!(bytes.len() < MAX_FRAME);
    let old = OperationIntent {
        schema_version: 1,
        id: "file-1".into(),
        plan: OperationPlan {
            project_id: None,
            action: PreparedAction::Copy {
                sources: vec![source(r"C:\file", EntryKind::File, index)],
                destination: source(r"D:\target", EntryKind::Folder, 2),
            },
        },
    };
    let bytes = serde_json::to_vec(&old).unwrap();
    serde_json::from_slice::<OperationIntent>(&bytes)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn tree_contract_preserves_result_count_and_serialized_intent_limits() {
    let large = plan(
        (0..MAX_OPERATION_RESULTS)
            .map(|index| {
                source(
                    &format!(r"C:\root\item-{index}"),
                    EntryKind::File,
                    index as u64 + 3,
                )
            })
            .collect(),
    );
    assert_eq!(large.validate().unwrap_err().code, FaultCode::ResourceLimit);
    let long = plan(
        (0..1000)
            .map(|index| {
                source(
                    &format!(r"C:\root\{}-{index}", "a".repeat(220)),
                    EntryKind::File,
                    index + 3,
                )
            })
            .collect(),
    );
    assert_eq!(long.validate().unwrap_err().code, FaultCode::PayloadLimit);
}
