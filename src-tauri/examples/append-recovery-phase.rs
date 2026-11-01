#[cfg(windows)]
fn main() {
    use projectpane_core::{
        domain::{MAX_FRAME, operations::recovery::RecoveryRecord},
        persistence::recovery::{AppendFailure, append_phase},
    };
    use std::{
        io::{self, Read},
        path::Path,
    };
    let result = (|| {
        let directory = std::env::args_os()
            .nth(1)
            .ok_or(AppendFailure::NotCreated(io::ErrorKind::InvalidInput))?;
        let mut bytes = Vec::new();
        let frame = u64::try_from(MAX_FRAME)
            .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
        std::io::stdin()
            .take(frame)
            .read_to_end(&mut bytes)
            .map_err(|error| AppendFailure::NotCreated(error.kind()))?;
        let record = RecoveryRecord::parse(&bytes)
            .map_err(|_| AppendFailure::NotCreated(io::ErrorKind::InvalidData))?;
        append_phase(Path::new(&directory), &record)
    })();
    match result {
        Ok(ack) => println!("{}", serde_json::json!(ack)),
        Err(error) => {
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
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
    std::process::exit(1);
}
