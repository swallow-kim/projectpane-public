use projectpane_core::domain::FaultCode;
use projectpane_core::{domain::Identity, services::Broker};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    },
};

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Worker(OwnedHandle);
impl Worker {
    fn open(pid: u32) -> Self {
        // SAFETY: OpenProcess takes a numeric PID and returns a new owned handle.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
        assert!(
            !raw.is_null(),
            "OpenProcess: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: the non-null handle is uniquely owned and closed by OwnedHandle.
        Self(unsafe { OwnedHandle::from_raw_handle(raw) })
    }

    fn exited(&self, millis: u32) -> bool {
        // SAFETY: the owned process handle remains live throughout the bounded wait.
        unsafe { WaitForSingleObject(self.0.as_raw_handle(), millis) == WAIT_OBJECT_0 }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: this is the retained handle of our fixture, never a reused PID.
        // Cleanup also runs on assertion failure, including the expected red test.
        if !self.exited(0) {
            // SAFETY: the retained owned handle has PROCESS_TERMINATE rights.
            unsafe { TerminateProcess(self.0.as_raw_handle(), 1) };
            if !self.exited(5_000) {
                eprintln!("fixture cleanup failed to terminate owned worker");
            }
        }
    }
}

fn fixture() {
    let mut signal =
        TcpStream::connect(std::env::var("PROJECTPANE_FIXTURE_ADDR").unwrap()).unwrap();
    signal
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    writeln!(signal, "{}", std::process::id()).unwrap();
    let mut ack = [0];
    signal.read_exact(&mut ack).unwrap();
    let mut request = String::new();
    if std::io::stdin().lock().read_line(&mut request).unwrap() == 0 {
        return;
    }
    // A real request has arrived. From here, stdin EOF cannot unblock the worker.
    let _: projectpane_core::domain::Request = serde_json::from_str(&request).unwrap();
    signal.write_all(b"B").unwrap();
    if std::env::var("PROJECTPANE_FIXTURE_MODE").as_deref() == Ok("crash") {
        signal.read_exact(&mut ack).unwrap();
        assert_eq!(ack, *b"X");
        std::process::exit(17);
    }
    loop {
        std::thread::park();
    }
}

fn parent(executable: &Path, mode: &str) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let broker = std::sync::Arc::new(Broker::new(
            executable.into(),
            Duration::from_secs(if mode == "timeout" { 2 } else { 60 }),
        ));
        let id = Identity {
            pane: 0,
            tab_id: "tab".into(),
            request_id: "request".into(),
            generation: 1,
        };
        let pending_broker = broker.clone();
        let pending_id = id.clone();
        let path = executable.parent().unwrap().to_string_lossy().into_owned();
        let pending = tokio::spawn(async move { pending_broker.start(pending_id, path).await });
        if mode == "cancel" {
            tokio::task::spawn_blocking(|| std::io::stdin().read_exact(&mut [0]))
                .await
                .unwrap()
                .unwrap();
            broker.cancel(&id).await.unwrap();
        }
        let fault = pending.await.unwrap().unwrap_err();
        assert_eq!(
            fault.code,
            if mode == "cancel" {
                FaultCode::Cancelled
            } else if mode == "crash" {
                FaultCode::ProtocolError
            } else {
                FaultCode::Timeout
            }
        );
        if mode == "crash" {
            assert_eq!(
                broker.read(id, None).await.unwrap_err().code,
                FaultCode::StaleRequest
            );
        }
    });
}

fn scenario(mode: &str) {
    // Given: a real Broker host and a fixture that blocks after the first request.
    let dir = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let fixture_path = dir.path().join("blocked-read-fixture.exe");
    std::fs::copy(&executable, &fixture_path).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut host = Host(
        Command::new(&executable)
            .arg("--fixture-parent")
            .arg(&fixture_path)
            .arg(mode)
            .env("PROJECTPANE_FIXTURE_MODE", mode)
            .env(
                "PROJECTPANE_FIXTURE_ADDR",
                listener.local_addr().unwrap().to_string(),
            )
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut signal = loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                break BufReader::new(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "fixture startup timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("fixture accept: {e}"),
        }
    };
    signal
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut pid = String::new();
    signal.read_line(&mut pid).unwrap();
    let worker = Worker::open(pid.trim().parse().unwrap());
    signal.get_mut().write_all(b"A").unwrap();
    let mut blocked = [0];
    signal.read_exact(&mut blocked).unwrap();
    assert_eq!(blocked, *b"B");
    assert!(!worker.exited(0));

    // When: forcibly terminate only the host, or request ordinary cancellation.
    if mode == "forced" {
        host.0.kill().unwrap();
        host.0.wait().unwrap();
    } else if mode == "cancel" {
        host.0.stdin.as_mut().unwrap().write_all(b"C").unwrap();
    } else if mode == "crash" {
        signal.get_mut().write_all(b"X").unwrap();
    }
    // Then: the blocked worker must exit; retain its handle for failure cleanup.
    let exited = worker.exited(5_000);
    eprintln!("{mode}: blocked worker {} exited={exited}", pid.trim());
    assert!(exited, "blocked worker survived {mode}");
    if mode != "forced" {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = host.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "host failed to reap worker");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if mode == "crash" {
        assert!(matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }
}

pub fn run() {
    let executable = std::env::current_exe().unwrap();
    if executable.file_name().unwrap() == "blocked-read-fixture.exe" {
        fixture();
        return;
    }
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture-parent") {
        parent(Path::new(&args[2]), args[3].to_str().unwrap());
        return;
    }
    for mode in ["forced", "cancel", "timeout", "crash"] {
        scenario(mode);
    }
}
