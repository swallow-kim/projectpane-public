use super::Broker;
use crate::domain::{Fault, Identity, ListingView, Reply, Request};

pub use crate::domain::DISPLAY_PAGE_SIZE;

impl Broker {
    pub async fn start_visible(&self, identity: Identity, path: String) -> Result<Reply, Fault> {
        let mut reply = self.start(identity, path).await?;
        reply.batch.entries.truncate(DISPLAY_PAGE_SIZE);
        Ok(reply)
    }

    pub async fn start_visible_view(
        &self,
        identity: Identity,
        path: String,
        view: ListingView,
    ) -> Result<Reply, Fault> {
        let mut reply = self.start_view(identity, path, view).await?;
        reply.batch.entries.truncate(DISPLAY_PAGE_SIZE);
        Ok(reply)
    }

    pub async fn read_visible(&self, identity: Identity, offset: usize) -> Result<Reply, Fault> {
        self.read_request(
            identity,
            Request::Window {
                offset,
                limit: DISPLAY_PAGE_SIZE,
            },
            self.deadline,
        )
        .await
    }
}
