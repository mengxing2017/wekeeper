use crate::{
    business::Actor,
    source_access::{
        domain::{Result, routing::Command},
        south::port::RoutingStore,
    },
};
#[derive(Clone)]
pub struct Service<R: RoutingStore>(pub R);
impl<R: RoutingStore> Service<R> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        self.0.execute(actor, command).await
    }
}
