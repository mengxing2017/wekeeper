use super::super::{domain::*, south::port::AdvertisingStore};
use crate::{
    business::Actor,
    source_access::domain::{Result, ensure},
};
pub struct Service<S>(pub S);
impl<S: AdvertisingStore> Service<S> {
    pub async fn execute(&self, actor: &Actor, command: Command) -> Result<serde_json::Value> {
        let revision = match &command {
            Command::Create(v) => {
                v.content.validate()?;
                ensure(v.workspaces.len() <= 100, "invalid_ad_scope")?;
                Some((0, v.reason.as_str()))
            }
            Command::Content(_, v) => {
                v.content.validate()?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::Policy(_, v) => {
                v.policy.validate()?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::Archive(_, v) => Some((v.expected_revision, v.reason.as_str())),
            Command::Configure(_, v) => {
                v.config.validate()?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::PublishPool(_, v) => {
                v.validate()?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::ResetPool(_, v) => {
                ensure(v.expected_cursor_version >= 0, "invalid_request")?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::CopyPool(_, v) => {
                ensure(!v.target_route_id.is_empty(), "invalid_request")?;
                Some((v.expected_revision, v.reason.as_str()))
            }
            Command::Consumptions(_, page)
            | Command::List { page, .. }
            | Command::Versions(_, page)
            | Command::Candidates(_, page)
            | Command::Compositions(_, _, page) => {
                ensure((1..=100000).contains(page), "invalid_request")?;
                None
            }
            Command::Preview(_, v) => {
                let link = Content {
                    name: "preview".into(),
                    body: "preview".into(),
                    link: Some(v.detail_url.clone()),
                };
                link.validate()?;
                ensure(
                    v.title.as_ref().map_or(0, String::len)
                        + v.content.as_ref().map_or(0, String::len)
                        <= 100000,
                    "invalid_request",
                )?;
                None
            }
            Command::Manager(_, v) => {
                ensure(v.expected_revision >= 0, "invalid_request")?;
                None
            }
            _ => None,
        };
        if let Some((r, reason)) = revision {
            ensure(
                r >= 0 && !reason.trim().is_empty() && reason.len() <= 2000,
                "invalid_request",
            )?;
        }
        self.0.execute(actor, command).await
    }
}
