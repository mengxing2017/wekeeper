use crate::{
    business::Actor,
    source_access::{
        domain::{
            Result, ensure,
            keyword_filter::{self, Command},
        },
        south::port::KeywordStore,
    },
};
pub struct Service<S>(pub S);
impl<S: KeywordStore> Service<S> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        match &command {
            Command::Publish { input, .. } => {
                ensure(
                    input.expected_revision >= 0
                        && !input.reason.trim().is_empty()
                        && input.reason.len() <= 2000,
                    "invalid_request",
                )?;
                keyword_filter::validate(&input.entries)?;
            }
            Command::Versions { page, .. }
            | Command::Checks { page, .. }
            | Command::Hits { page, .. } => ensure((1..=100000).contains(page), "invalid_request")?,
            _ => {}
        }
        self.0.execute(actor, command).await
    }
}
