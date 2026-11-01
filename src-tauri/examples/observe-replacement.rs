#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use projectpane_core::{
        domain::{MAX_FRAME, operations::recovery::RecoveryRecord},
        platform::recovery_observation::observe_replacement,
    };
    use std::io::Read;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(u64::try_from(MAX_FRAME)?)
        .read_to_end(&mut bytes)?;
    match RecoveryRecord::parse(&bytes) {
        Ok(record) => println!("{}", serde_json::to_string(&observe_replacement(&record))?),
        Err(fault) => {
            eprintln!("{}", serde_json::to_string(&fault)?);
            std::process::exit(1);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
    std::process::exit(1);
}
