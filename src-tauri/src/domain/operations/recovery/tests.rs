use super::{RecoveryAssessment as A, inspect_replacement};
use crate::domain::{FaultCode, MAX_FRAME};
use serde_json::{Value, json};

fn fixture() -> Value {
    let object = |path: &str, index: u64, kind: &str| {
        json!({
            "path": path, "volume": 7, "fileIndex": index, "kind": kind,
        })
    };
    let content = json!({"bytes": 4, "sha256": vec![9; 32]});
    let original_content = json!({"bytes": 8, "sha256": vec![3; 32]});
    let record = json!({
        "schemaVersion": 1, "id": "proof-1", "phase": "ready",
        "source": {"object": object("D:\\source\\한글.txt", u64::MAX, "file"), "content": content},
        "original": {"object": object("D:\\target\\한글.txt", 2, "file"), "content": original_content},
        "staged": {"object": object("D:\\target\\.projectpane-recovery-proof-1\\staged", 3, "file"), "content": content},
        "parent": object("D:\\target", 4, "folder"),
        "artifacts": object("D:\\target\\.projectpane-recovery-proof-1", 5, "folder"),
        "backupPath": "D:\\target\\.projectpane-recovery-proof-1\\original",
    });
    let mut value = json!({"record": record, "observations": {}});
    value["observations"] = json!({
        "parent": observation(&value["record"]["parent"], Value::Null),
        "artifacts": observation(&value["record"]["artifacts"], Value::Null),
        "target": observed_file(&value["record"]["original"]),
        "staged": observed_file(&value["record"]["staged"]),
        "backup": {"state": "absent"},
    });
    value
}

fn observation(object: &Value, content: Value) -> Value {
    json!({"state": "present", "kind": object["kind"], "volume": object["volume"],
        "fileIndex": object["fileIndex"], "content": content})
}

fn observed_file(file: &Value) -> Value {
    observation(&file["object"], file["content"].clone())
}

fn inspect(value: &Value) -> A {
    inspect_replacement(&serde_json::to_vec(value).unwrap()).unwrap()
}

#[test]
fn phase_and_actual_layout_must_agree_without_inventing_completion() {
    for (phase, before, preserved, installed) in [
        ("ready", A::OriginalAtTarget, A::Unknown, A::Unknown),
        (
            "preserve_requested",
            A::OriginalAtTarget,
            A::OriginalRetained,
            A::Unknown,
        ),
        (
            "original_preserved",
            A::Unknown,
            A::OriginalRetained,
            A::Unknown,
        ),
        (
            "install_requested",
            A::Unknown,
            A::OriginalRetained,
            A::InstalledWithOriginalRetained,
        ),
        (
            "install_observed",
            A::Unknown,
            A::Unknown,
            A::InstalledWithOriginalRetained,
        ),
    ] {
        let mut value = fixture();
        value["record"]["phase"] = json!(phase);
        assert_eq!(inspect(&value), before, "{phase}: original at target");
        value["observations"]["backup"] = observed_file(&value["record"]["original"]);
        value["observations"]["target"] = json!({"state": "absent"});
        assert_eq!(inspect(&value), preserved, "{phase}: original retained");
        value["observations"]["target"] = observed_file(&value["record"]["staged"]);
        value["observations"]["staged"] = json!({"state": "absent"});
        assert_eq!(inspect(&value), installed, "{phase}: staged at target");
    }
}

#[test]
fn collision_retains_original_but_inaccessible_destination_stays_unknown() {
    let mut value = fixture();
    value["record"]["phase"] = json!("install_requested");
    value["observations"]["backup"] = observed_file(&value["record"]["original"]);
    value["observations"]["target"]["fileIndex"] = json!(99);
    assert_eq!(inspect(&value), A::DestinationOccupiedOriginalRetained);
    value["observations"]["target"] = json!({"state": "inaccessible"});
    assert_eq!(inspect(&value), A::Unknown);
}

#[test]
fn content_identity_and_directory_ambiguity_never_authorize_recovery() {
    for slot in ["parent", "artifacts", "target", "staged"] {
        for state in ["absent", "inaccessible"] {
            let mut value = fixture();
            value["observations"][slot] = json!({"state": state});
            assert_eq!(inspect(&value), A::Unknown, "{slot}: {state}");
        }
        let mut value = fixture();
        value["observations"][slot]["fileIndex"] = json!(99);
        assert_eq!(inspect(&value), A::Unknown, "{slot}: different identity");
    }
    for slot in ["target", "staged"] {
        for field in ["bytes", "sha256"] {
            let mut value = fixture();
            value["observations"][slot]["content"][field] = if field == "bytes" {
                json!(123)
            } else {
                json!(vec![5; 32])
            };
            assert_eq!(
                inspect(&value),
                A::Unknown,
                "{slot}: {field} changed on same file"
            );
        }
    }
}

#[test]
fn duplicate_observed_identity_is_ambiguous_even_with_different_content() {
    for slot in ["backup", "staged", "parent", "artifacts"] {
        let mut value = fixture();
        value["record"]["phase"] = json!("install_requested");
        value["observations"]["backup"] = observed_file(&value["record"]["original"]);
        value["observations"]["target"] = value["observations"][slot].clone();
        if matches!(slot, "backup" | "staged") {
            value["observations"]["target"]["content"]["bytes"] = json!(99);
        }
        assert_eq!(inspect(&value), A::Unknown, "target aliases {slot}");
    }
}

#[test]
fn malformed_or_ambiguous_record_is_rejected_at_the_boundary() {
    let baseline = fixture();
    for (pointer, replacement) in [
        ("/record/schemaVersion", json!(2)),
        ("/record/id", json!("../escape")),
        ("/record/phase", json!("success")),
        ("/record/extra", json!(true)),
        ("/record/backupPath", json!("D:\\target\\existing.txt")),
        ("/record/parent/path", json!("D:\\target\\..\\target")),
        ("/record/source/object/fileIndex", json!(2)),
        ("/record/source/object/volume", json!(8)),
        ("/record/source/object/kind", json!("folder")),
        ("/record/staged/content/bytes", json!(2)),
        ("/record/source/content/sha256", json!([1, 2])),
        ("/observations/target/fileIndex", json!(0)),
        ("/observations/target/content", Value::Null),
        (
            "/observations/parent/content",
            json!({"bytes": 4, "sha256": vec![9; 32]}),
        ),
    ] {
        let mut value = baseline.clone();
        if pointer == "/record/extra" {
            value["record"]["extra"] = replacement;
        } else {
            *value.pointer_mut(pointer).unwrap() = replacement;
        }
        assert!(
            inspect_replacement(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut value = baseline;
    value["observations"]
        .as_object_mut()
        .unwrap()
        .remove("backup");
    assert!(inspect_replacement(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn bounded_input_and_lossless_native_integer_identity() {
    let value = fixture();
    assert_eq!(
        value["record"]["source"]["object"]["fileIndex"].as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(inspect(&value), A::OriginalAtTarget);
    assert_eq!(
        inspect_replacement(&vec![b' '; MAX_FRAME])
            .unwrap_err()
            .code,
        FaultCode::PayloadLimit
    );
    assert!(inspect_replacement(b"{broken").is_err());
}
