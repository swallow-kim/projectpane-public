use super::*;

pub(super) async fn fixture(reply: &str) -> (Session, tempfile::NamedTempFile) {
    use std::io::Write;
    let mut reply_file = tempfile::NamedTempFile::new().unwrap();
    writeln!(reply_file, "{reply}").unwrap();
    reply_file.flush().unwrap();
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd.exe");
        command.as_std_mut().raw_arg(format!(
            "/D /S /C \"set /P request= & type \"{}\" & set /P blocked=\"",
            reply_file.path().display()
        ));
        command.creation_flags(0x00000008);
        command
    };
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "read request; cat \"$1\"; read blocked",
            "fixture",
            reply_file.path().to_str().unwrap(),
        ]);
        command
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    #[cfg(windows)]
    let job = {
        let job = worker_job::WorkerJob::new().unwrap();
        job.assign(&mut child).await.unwrap();
        job
    };
    let input = child.stdin.take().unwrap();
    let output = BufReader::new(child.stdout.take().unwrap());
    let session = Session {
        identity: Identity {
            pane: 0,
            tab_id: "test".into(),
            request_id: "test-1".into(),
            generation: 1,
        },
        pipes: AsyncMutex::new(Pipes {
            child,
            input,
            output,
            #[cfg(windows)]
            _job: job,
        }),
        cancelled: watch::channel(false).0,
    };
    (session, reply_file)
}

#[tokio::test]
async fn malformed_replies_are_protocol_errors_and_reaped_before_return() {
    for reply in [
        r#"{"Err":{"code":"unknown-private-value"}}"#,
        "not-json",
        r#"{"Ok":{"revision":1,"offset":0,"entries":[],"total":0,"complete":true,"capped":false,"snapshotBytes":0}}"#,
        r#"{"Ok":{"revision":1,"offset":1,"entries":[],"total":0,"complete":true,"capped":false,"snapshotBytes":0,"capReason":null}}"#,
        r#"{"Ok":{"revision":1,"offset":0,"entries":[],"total":100001,"complete":true,"capped":false,"snapshotBytes":0,"capReason":null}}"#,
        r#"{"Ok":{"revision":1,"offset":0,"entries":[],"total":0,"complete":true,"capped":true,"snapshotBytes":0,"capReason":"unknown"}}"#,
    ] {
        let (session, _reply_file) = fixture(reply).await;
        let result = session
            .exchange(Request::Progress, Duration::from_secs(8))
            .await;
        assert_eq!(result.unwrap_err().code, FaultCode::ProtocolError);
        let mut pipes = session.pipes.lock().await;
        assert_eq!(pipes.child.id(), None);
        assert!(pipes.child.try_wait().unwrap().is_some());
        assert!(*session.cancelled.borrow());
    }
}
