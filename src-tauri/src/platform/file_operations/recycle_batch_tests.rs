use super::{NativeFileOperationLane, capture_source};
use crate::domain::{FaultCode, operations::*};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};

#[tokio::test]
async fn unsupported_folder_rejects_entire_recycle_batch_before_changing_first_file() {
    let fixture = tempfile::tempdir().unwrap();
    let file = fixture.path().join("first.txt");
    let folder = fixture.path().join("unsupported-folder");
    fs::write(&file, b"first source must survive").unwrap();
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("child.txt"), b"nested source must survive").unwrap();
    let sources = [&file, &folder]
        .into_iter()
        .map(|path| capture_source(path.to_str().unwrap()).unwrap())
        .collect();
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(
            OperationPlan {
                project_id: None,
                action: PreparedAction::Recycle { sources },
            },
            0,
            Arc::new(AtomicBool::new(false)),
        )
        .await;
    lane.shutdown().await.unwrap();
    assert_eq!(result.unwrap_err().code, FaultCode::InvalidRequest);
    assert_eq!(fs::read(&file).unwrap(), b"first source must survive");
    assert_eq!(
        fs::read(folder.join("child.txt")).unwrap(),
        b"nested source must survive"
    );
}
