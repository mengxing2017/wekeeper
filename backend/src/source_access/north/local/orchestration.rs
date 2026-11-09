use crate::{
    business::Actor,
    source_access::{
        domain::{Result, ensure, orchestration::Command},
        south::port::OrchestrationStore,
    },
};
#[derive(Clone)]
pub struct Service<R>(pub R);
impl<R: OrchestrationStore> Service<R> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        match &command {
            Command::List(f) => ensure(
                (1..=100_000).contains(&f.page.unwrap_or(1))
                    && f.from.zip(f.until).is_none_or(|(a, b)| a <= b),
                "invalid_request",
            )?,
            Command::Stop(_, c) | Command::Retry(_, c) => c.validate()?,
            Command::Events(_, after) => ensure(*after >= 0, "invalid_request")?,
            Command::Policy(_, Some(p)) => ensure(
                p.revision >= 0 && !p.reason.trim().is_empty() && p.reason.chars().count() <= 2000,
                "invalid_request",
            )?,
            _ => {}
        }
        self.0.execute(actor, command).await
    }
}
