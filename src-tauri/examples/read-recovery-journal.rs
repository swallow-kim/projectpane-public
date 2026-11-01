use projectpane_core::{
    domain::operations::recovery::RecoveryPhase, persistence::recovery::read_journal,
};
use serde::Serialize;
use std::{io, path::Path};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary<'a> {
    id: &'a str,
    phase: RecoveryPhase,
    requires_observation: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "directory_required"))?;
    match read_journal(Path::new(&directory)) {
        Ok(records) => {
            let summaries: Vec<_> = records
                .iter()
                .map(|record| Summary {
                    id: record.id(),
                    phase: record.phase(),
                    requires_observation: true,
                })
                .collect();
            println!("{}", serde_json::to_string(&summaries)?);
        }
        Err(error) => {
            eprintln!("{:?}", error.kind());
            std::process::exit(1);
        }
    }
    Ok(())
}
