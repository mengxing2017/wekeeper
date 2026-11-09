use crate::{
    business::Actor,
    source_access::{
        domain::{Result, routing::*},
        south::port::RoutingStore,
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: RoutingStore>(pub R);
impl<R: RoutingStore> Service<R> {
    pub async fn execute(&self, actor: &Actor, mut command: Command) -> Result<Value> {
        match &mut command {
            Command::Entity { input, .. } => input.validate()?,
            Command::Configure { input, .. } => {
                if let Some(c) = &mut input.execution {
                    c.normalize()?;
                }
            }
            Command::SetGroupTargets { input, .. } => input.execution.normalize()?,
            Command::Lifecycle { input, .. } => input.validate()?,
            _ => {}
        }
        self.0.execute(actor, command).await
    }
    pub async fn resolve(&self, receipt: Uuid) -> Result<Resolution> {
        self.0.resolve(receipt).await
    }
}
