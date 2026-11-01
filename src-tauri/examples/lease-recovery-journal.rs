#[cfg(windows)]
fn run() -> Result<(), projectpane_core::persistence::recovery::AppendFailure> {
    use projectpane_core::{
        domain::{MAX_FRAME, operations::recovery::RecoveryRecord},
        persistence::recovery::{AppendFailure, JournalLease},
    };
    use std::io::{self, BufRead, Read, Write};
    let before = |error: io::Error| AppendFailure::NotCreated(error.kind());
    let directory = std::env::args_os()
        .nth(1)
        .ok_or(AppendFailure::NotCreated(io::ErrorKind::InvalidInput))?;
    let mut lease = JournalLease::open(std::path::Path::new(&directory))?;
    let input = io::stdin();
    let mut input = input.lock();
    let output = io::stdout();
    let mut output = output.lock();
    writeln!(output, "{}", serde_json::json!({"event":"locked"})).map_err(before)?;
    output.flush().map_err(before)?;
    let frame = u64::try_from(MAX_FRAME)
        .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
    loop {
        let mut bytes = Vec::new();
        let count = (&mut input)
            .take(frame)
            .read_until(b'\n', &mut bytes)
            .map_err(before)?;
        if count == 0 {
            drop(lease);
            writeln!(output, "{}", serde_json::json!({"event":"released"})).map_err(before)?;
            return output.flush().map_err(before);
        }
        let record = RecoveryRecord::parse(&bytes)
            .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
        let (next, ack) = lease.append(&record)?;
        lease = next;
        // A successful append followed by broken stdout still has a journal effect.
        let after = |error: io::Error| AppendFailure::Unacknowledged(error.kind());
        writeln!(output, "{}", serde_json::json!({"event":"ack","ack":ack})).map_err(after)?;
        output.flush().map_err(after)?;
    }
}

#[cfg(windows)]
fn main() {
    use projectpane_core::persistence::recovery::AppendFailure;
    if let Err(error) = run() {
        let (effect, kind) = match error {
            AppendFailure::NotCreated(kind) => ("not_created", kind),
            AppendFailure::Unacknowledged(kind) => ("unacknowledged", kind),
        };
        eprintln!(
            "{}",
            serde_json::json!({"effect":effect,"kind":format!("{kind:?}")})
        );
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
    std::process::exit(1);
}
