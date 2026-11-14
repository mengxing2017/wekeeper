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
        if let Command::List { page, .. } = &command {
            ensure((1..=100_000).contains(page), "invalid_request")?;
        }
        self.0.execute(actor, command).await
    }
}
