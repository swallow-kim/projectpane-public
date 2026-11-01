#[cfg(windows)]
#[tokio::main]
async fn main() {
    use projectpane_core::{
        domain::{Fault, FaultCode, operations::*},
        platform::file_operations::{NativeFileOperationLane, capture_source, check_plan},
    };
    use std::{path::Path, sync::Arc};

    let prepare = || -> Result<OperationPlan, Fault> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if !(2..=4).contains(&args.len()) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let destination = capture_source(&args[0])?;
        let sources = args[1..]
            .iter()
            .map(|path| capture_source(path))
            .collect::<Result<Vec<_>, _>>()?;
        let plan = OperationPlan {
            project_id: None,
            action: PreparedAction::Copy {
                sources,
                destination,
            },
        };
        check_plan(&plan)?;
        Ok(plan)
    };
    let result = async {
        let plan = prepare()?;
        let lane = NativeFileOperationLane::start()?;
        let result = lane.execute(plan, 0, Arc::default()).await;
        lane.shutdown().await?;
        result
    }
    .await;
    match result {
        Ok(report) => {
            let completion = OperationCompletion {
                schema_version: 1,
                id: "native-permission-copy".into(),
                report: report.clone(),
            };
            if let Err(error) = completion.validate() {
                eprintln!("{}", serde_json::json!({"validationFault": error.code}));
                std::process::exit(1);
            }
            let items: Vec<_> = report
                .items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "sourceName": Path::new(&item.source).file_name().and_then(|s| s.to_str()),
                        "destinationName": item.destination.as_deref().and_then(|p| Path::new(p).file_name()).and_then(|s| s.to_str()),
                        "status": item.status, "hresult": item.hresult,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({"status":report.status(),"counts":report.counts,
                    "evidence":report.evidence,"failure":report.failure,"items":items,
                    "completionValid":true,"laneJoined":true})
            );
        }
        Err(error) => {
            eprintln!("{}", serde_json::json!({"fault":error.code}));
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
    std::process::exit(1);
}
