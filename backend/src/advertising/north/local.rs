use super::super::domain::*;
use crate::business::Actor;

fn ensure(ok: bool, code: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(AdvertisingError(code))
    }
}

pub struct AdDecisionService<S>(pub S);
impl<S: super::super::south::port::AdDecisionStore> AdDecisionService<S> {
    pub async fn decide(&self, input: decision::DecisionInput) -> Result<decision::Decision> {
        ensure(
            !input.source.platform_id.is_nil()
                && !input.source.unit_id.is_nil()
                && !input.message_id.is_nil()
                && !input.original.trim().is_empty()
                && matches!(input.channel.as_str(), "mobile" | "wecom")
                && measure(&input.original, &input.channel).valid,
            "invalid_request",
        )?;
        self.0.decide(input).await
    }
}

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

pub struct AdPlacementService<S>(pub S);
impl<S: super::super::south::port::AdPlacementStore> AdPlacementService<S> {
    pub async fn create(
        &self,
        actor: &Actor,
        input: targeting::CreateAdPlacement,
    ) -> Result<targeting::AdPlacementView> {
        use targeting::TargetScope;
        ensure(actor.platform_admin, "forbidden")?;
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
        self.0.create_placement(actor, input).await
    }
    pub async fn get(&self, actor: &Actor, id: uuid::Uuid) -> Result<targeting::AdPlacementView> {
        ensure(actor.platform_admin, "forbidden")?;
        self.0
            .placement(actor, id)
            .await?
            .ok_or(AdvertisingError("not_found"))
    }
    pub async fn list(
        &self,
        actor: &Actor,
        page: i64,
    ) -> Result<targeting::ManagementPage<targeting::AdPlacementView>> {
        let offset = management_offset(actor, page)?;
        Ok(management_page(self.0.placements(actor, offset).await?))
    }
    pub async fn set_state(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: targeting::SetPlacementState,
    ) -> Result<targeting::AdPlacementView> {
        ensure(actor.platform_admin, "forbidden")?;
        ensure(input.expected_revision > 0, "invalid_request")?;
        ensure(
            !input.reason.trim().is_empty() && input.reason.chars().count() <= 2000,
            "invalid_request",
        )?;
        self.0.set_placement_state(actor, id, input).await
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
pub struct MatchingAds<S>(pub S);
impl<S: super::super::south::port::AdPlacementReader> MatchingAds<S> {
    /// Read all discovery pages before resolving scope precedence. Retain all
    /// ties; this is not a frozen decision or a reservation of advertising slots.
    pub async fn preferred_candidates(
        &self,
        identity: targeting::SourceIdentity,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<targeting::ScopeMatch>> {
        let mut candidates = Vec::new();
        let mut after = None;
        loop {
            let page = self.page(identity, at, after).await?;
            candidates.extend(page.items);
            candidates = targeting::preferred_candidates(identity, candidates);
            match page.next_after {
                Some(cursor) => after = Some(cursor),
                None => return Ok(candidates),
            }
        }
    }

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
