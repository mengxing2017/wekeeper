use super::super::domain::{Command, Result};
use crate::business::Actor;

pub trait AdvertisingStore: Clone + Send + Sync {
    fn execute(
        &self,
        actor: &Actor,
        command: Command,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send;
}

/// Read-only matching. Final authorization and selection require their own use case.
pub trait TargetRuleReader: Send + Sync {
    fn matching_page(
        &self,
        identity: crate::advertising::domain::targeting::SourceIdentity,
        at: chrono::DateTime<chrono::Utc>,
        after: Option<uuid::Uuid>,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::ScopeMatchPage>,
    > + Send;
}

/// Management of independent materials; authorization is enforced by the service.
pub trait MaterialStore: Send + Sync {
    fn create_material(
        &self,
        actor: &Actor,
        input: crate::advertising::domain::targeting::CreateMaterial,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::MaterialView>,
    > + Send;
    fn material(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
    ) -> impl std::future::Future<
        Output = Result<Option<crate::advertising::domain::targeting::MaterialView>>,
    > + Send;
    fn materials(
        &self,
        actor: &Actor,
        offset: i64,
    ) -> impl std::future::Future<
        Output = Result<Vec<crate::advertising::domain::targeting::MaterialView>>,
    > + Send;
    fn material_versions(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        offset: i64,
    ) -> impl std::future::Future<
        Output = Result<Option<Vec<crate::advertising::domain::targeting::MaterialVersionView>>>,
    > + Send;
    fn archive_material(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: crate::advertising::domain::targeting::ArchiveMaterial,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::MaterialView>,
    > + Send;
    fn publish_material(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: crate::advertising::domain::targeting::PublishMaterial,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::MaterialView>,
    > + Send;
}

pub trait PlacementRuleStore: Send + Sync {
    fn create_rule(
        &self,
        actor: &Actor,
        input: crate::advertising::domain::targeting::CreatePlacementRule,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::PlacementRuleView>,
    > + Send;
    fn rule(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
    ) -> impl std::future::Future<
        Output = Result<Option<crate::advertising::domain::targeting::PlacementRuleView>>,
    > + Send;
    fn rules(
        &self,
        actor: &Actor,
        offset: i64,
    ) -> impl std::future::Future<
        Output = Result<Vec<crate::advertising::domain::targeting::PlacementRuleView>>,
    > + Send;
    fn set_rule_state(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: crate::advertising::domain::targeting::SetPlacementState,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::PlacementRuleView>,
    > + Send;
}
