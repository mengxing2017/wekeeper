use crate::{
    business::Actor,
    source_access::{
        domain::{Fault, Result, projection::Command},
        south::port::ProjectionStore,
    },
};
#[derive(Clone)]
pub struct Service<R: ProjectionStore>(pub R);
impl<R: ProjectionStore> Service<R> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        if !actor.platform_admin {
            return Err(Fault("forbidden"));
        }
        let Command::List { page, .. } = &command;
        page.bounds().map_err(|_| Fault("invalid_page"))?;
        self.0.execute(actor, command).await
    }
}
