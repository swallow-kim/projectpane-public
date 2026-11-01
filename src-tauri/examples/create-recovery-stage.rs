#[cfg(windows)]
fn main() {
    use projectpane_core::platform::recovery_observation::staging::{StageFailure, create_stage};
    let result = (|| {
        let mut args = std::env::args().skip(1);
        let source = args
            .next()
            .ok_or(StageFailure::NotCreated(std::io::ErrorKind::InvalidInput))?;
        let destination = args
            .next()
            .ok_or(StageFailure::NotCreated(std::io::ErrorKind::InvalidInput))?;
        create_stage(&source, &destination)
    })();
    match result {
        Ok(staged) => println!("{}", serde_json::json!(staged.receipt())),
        Err(error) => {
            let (effect, kind) = match error {
                StageFailure::NotCreated(kind) => ("not_created", kind),
                StageFailure::Unacknowledged(kind) => ("unacknowledged", kind),
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
