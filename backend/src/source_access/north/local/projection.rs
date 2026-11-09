use crate::{
    business::Actor,
    source_access::{
        domain::{Result, ensure, projection::Command},
        south::port::ProjectionStore,
    },
};
#[derive(Clone)]
pub struct Service<R: ProjectionStore>(pub R);
impl<R: ProjectionStore> Service<R> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        match &command {
            Command::List { page, .. } => ensure((1..=100_000).contains(page), "invalid_request")?,
            Command::Recheck { input, .. } => ensure(
                input.revision >= 0
                    && !input.reason.trim().is_empty()
                    && input.reason.len() <= 2000,
                "invalid_request",
            )?,
            _ => {}
        }
        self.0.execute(actor, command).await
    }
}

/// Drives projection, checks and channel handoff through a durable port.
pub struct Worker<S>(pub S);
impl<S: crate::source_access::south::port::InboxWorker> Worker<S> {
    pub async fn run_once(&self) -> Result<()> {
        self.0.tick().await
    }
}
