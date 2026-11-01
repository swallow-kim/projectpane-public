use super::{
    tests::{captured, object},
    *,
};
use serde_json::json;

#[test]
fn observes_prearranged_retained_occupied_and_installed_layouts() {
    for (layout, expected) in [
        ("retained", RecoveryAssessment::OriginalRetained),
        (
            "occupied",
            RecoveryAssessment::DestinationOccupiedOriginalRetained,
        ),
        (
            "installed",
            RecoveryAssessment::InstalledWithOriginalRetained,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("target");
        let artifacts = parent.join(".projectpane-recovery-proof-1");
        fs::create_dir_all(&artifacts).unwrap();
        let source = root.path().join("source");
        let target = parent.join("한글.txt");
        let staged = artifacts.join("staged");
        let backup = artifacts.join("original");
        fs::write(&source, b"abc").unwrap();
        fs::write(&backup, b"original").unwrap();
        let installed = layout == "installed";
        fs::write(if installed { &target } else { &staged }, b"abc").unwrap();
        if layout == "occupied" {
            fs::write(&target, b"external").unwrap();
        }
        let mut original_record = captured(&backup);
        original_record["object"]["path"] = json!(target);
        let mut staged_record = captured(if installed { &target } else { &staged });
        staged_record["object"]["path"] = json!(staged);
        let value = json!({"schemaVersion":1,"id":"proof-1","phase":"install_requested",
            "source":captured(&source),"original":original_record,"staged":staged_record,
            "parent":object(&parent),"artifacts":object(&artifacts),"backupPath":backup});
        let record = RecoveryRecord::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(observe_replacement(&record), expected, "{layout}");
        assert_eq!(fs::read(&backup).unwrap(), b"original");
        assert_eq!(fs::read(&source).unwrap(), b"abc");
        if layout == "occupied" {
            assert_eq!(fs::read(&target).unwrap(), b"external");
        }
    }
}
