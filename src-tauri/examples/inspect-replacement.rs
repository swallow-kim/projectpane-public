use projectpane_core::domain::{MAX_FRAME, operations::recovery::inspect_replacement};
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(u64::try_from(MAX_FRAME)?)
        .read_to_end(&mut bytes)?;
    match inspect_replacement(&bytes) {
        Ok(assessment) => println!("{}", serde_json::to_string(&assessment)?),
        Err(fault) => {
            eprintln!("{}", serde_json::to_string(&fault)?);
            std::process::exit(1);
        }
    }
    Ok(())
}
