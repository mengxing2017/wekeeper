use super::super::{domain::*, south::port::AdvertisingStore};
use crate::business::Actor;

fn ensure(ok: bool, code: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(AdvertisingError(code))
    }
}
pub struct Service<S>(pub S);

pub struct MaterialService<S>(pub S);
impl<S: super::super::south::port::MaterialStore> MaterialService<S> {
    pub async fn create(
        &self,
        actor: &Actor,
        input: targeting::CreateMaterial,
    ) -> Result<targeting::MaterialView> {
        ensure(actor.platform_admin, "forbidden")?;
        validate_material(&input.content, &input.reason)?;
        self.0.create_material(actor, input).await
    }
    pub async fn get(&self, actor: &Actor, id: uuid::Uuid) -> Result<targeting::MaterialView> {
        ensure(actor.platform_admin, "forbidden")?;
        self.0
            .material(actor, id)
            .await?
            .ok_or(AdvertisingError("not_found"))
    }
    pub async fn list(
        &self,
        actor: &Actor,
        page: i64,
    ) -> Result<targeting::ManagementPage<targeting::MaterialView>> {
        let offset = management_offset(actor, page)?;
        Ok(management_page(self.0.materials(actor, offset).await?))
    }
    pub async fn versions(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        page: i64,
    ) -> Result<targeting::ManagementPage<targeting::MaterialVersionView>> {
        let offset = management_offset(actor, page)?;
        let versions = self
            .0
            .material_versions(actor, id, offset)
            .await?
            .ok_or(AdvertisingError("not_found"))?;
        Ok(management_page(versions))
    }
    pub async fn archive(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: targeting::ArchiveMaterial,
    ) -> Result<targeting::MaterialView> {
        ensure(actor.platform_admin, "forbidden")?;
        ensure(
            input.expected_revision > 0
                && !input.reason.trim().is_empty()
                && input.reason.chars().count() <= 2000,
            "invalid_request",
        )?;
        self.0.archive_material(actor, id, input).await
    }
    pub async fn publish(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: targeting::PublishMaterial,
    ) -> Result<targeting::MaterialView> {
        ensure(actor.platform_admin, "forbidden")?;
        ensure(input.expected_revision > 0, "invalid_request")?;
        validate_material(&input.content, &input.reason)?;
        self.0.publish_material(actor, id, input).await
    }
}

fn validate_material(content: &Content, reason: &str) -> Result<()> {
    content.validate()?;
    ensure(
        !reason.trim().is_empty() && reason.chars().count() <= 2000,
        "invalid_request",
    )
}

pub struct PlacementRuleService<S>(pub S);
impl<S: super::super::south::port::PlacementRuleStore> PlacementRuleService<S> {
    pub async fn create(
        &self,
        actor: &Actor,
        input: targeting::CreatePlacementRule,
    ) -> Result<targeting::PlacementRuleView> {
        use targeting::TargetScope;
        ensure(actor.platform_admin, "forbidden")?;
        ensure(
            !input.name.trim().is_empty() && input.name.trim().chars().count() <= 120,
            "invalid_request",
        )?;
        ensure(
            !input.reason.trim().is_empty() && input.reason.chars().count() <= 2000,
            "invalid_request",
        )?;
        ensure(
            !input.material_id.is_nil()
                && input.valid_until.is_none_or(|end| end > input.valid_from),
            "invalid_request",
        )?;
        let valid_scope = match input.scope {
            TargetScope::Global {} => true,
            TargetScope::Platform { platform_id } => !platform_id.is_nil(),
            TargetScope::Unit {
                platform_id,
                unit_id,
            } => !platform_id.is_nil() && !unit_id.is_nil(),
        };
        ensure(valid_scope, "invalid_ad_scope")?;
        self.0.create_rule(actor, input).await
    }
    pub async fn get(&self, actor: &Actor, id: uuid::Uuid) -> Result<targeting::PlacementRuleView> {
        ensure(actor.platform_admin, "forbidden")?;
        self.0
            .rule(actor, id)
            .await?
            .ok_or(AdvertisingError("not_found"))
    }
    pub async fn list(
        &self,
        actor: &Actor,
        page: i64,
    ) -> Result<targeting::ManagementPage<targeting::PlacementRuleView>> {
        let offset = management_offset(actor, page)?;
        Ok(management_page(self.0.rules(actor, offset).await?))
    }
    pub async fn set_state(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: targeting::SetPlacementState,
    ) -> Result<targeting::PlacementRuleView> {
        ensure(actor.platform_admin, "forbidden")?;
        ensure(input.expected_revision > 0, "invalid_request")?;
        ensure(
            !input.reason.trim().is_empty() && input.reason.chars().count() <= 2000,
            "invalid_request",
        )?;
        self.0.set_rule_state(actor, id, input).await
    }
}

fn management_offset(actor: &Actor, page: i64) -> Result<i64> {
    ensure(actor.platform_admin, "forbidden")?;
    ensure((1..=100_000).contains(&page), "invalid_request")?;
    Ok((page - 1) * 30)
}

fn management_page<T>(mut items: Vec<T>) -> targeting::ManagementPage<T> {
    let has_more = items.len() > 30;
    items.truncate(30);
    targeting::ManagementPage { items, has_more }
}

/// Read-only discovery; callers still need to authorize and decide delivery.
/// Pages read current versions independently and are not a transactional snapshot.
pub struct MatchingRules<S>(pub S);
impl<S: super::super::south::port::TargetRuleReader> MatchingRules<S> {
    pub async fn page(
        &self,
        identity: targeting::SourceIdentity,
        at: chrono::DateTime<chrono::Utc>,
        after: Option<uuid::Uuid>,
    ) -> Result<targeting::ScopeMatchPage> {
        ensure(
            !identity.platform_id.is_nil() && !identity.unit_id.is_nil(),
            "invalid_request",
        )?;
        self.0.matching_page(identity, at, after).await
    }
}
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
