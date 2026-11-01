use super::Broker;
use crate::domain::{Fault, Identity, Reply, Request};
use std::time::{Duration, Instant};

const MAX_PROGRESS_CHUNKS: usize = 512;
const PROGRESS_SLICE: Duration = Duration::from_millis(50);

impl Broker {
    pub async fn progress(&self, identity: Identity) -> Result<Reply, Fault> {
        let started = Instant::now();
        let mut reply = self
            .read_request(identity.clone(), Request::Progress, self.deadline)
            .await?;
        let offset = reply.batch.offset;
        // Coalesce metadata only; data frames remain bounded to 256 rows.
        // The cooperative slice is checked between exchanges, so a blocked OS
        // call can exceed it. Every exchange retains the original RPC deadline.
        for _ in 1..MAX_PROGRESS_CHUNKS {
            if reply.batch.complete || started.elapsed() >= PROGRESS_SLICE {
                break;
            }
            reply = self
                .read_request(
                    identity.clone(),
                    Request::Progress,
                    self.deadline.saturating_sub(started.elapsed()),
                )
                .await?;
        }
        reply.batch.offset = offset.min(reply.batch.total);
        Ok(reply)
    }
}
