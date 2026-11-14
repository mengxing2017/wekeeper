use super::super::domain::Result;
use crate::business::Actor;

/// Trusted immediate-message use case; no configuration-version or revocation flow.
pub trait AdDecisionStore: Send + Sync {
    fn decide(
        &self,
        input: crate::advertising::domain::decision::DecisionInput,
    ) -> impl std::future::Future<Output = Result<crate::advertising::domain::decision::Decision>> + Send;
}

/// Read-only matching. Final authorization and selection require their own use case.
pub trait AdPlacementReader: Send + Sync {
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

pub trait AdPlacementStore: Send + Sync {
    fn create_placement(
        &self,
        actor: &Actor,
        input: crate::advertising::domain::targeting::CreateAdPlacement,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::AdPlacementView>,
    > + Send;
    fn placement(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
    ) -> impl std::future::Future<
        Output = Result<Option<crate::advertising::domain::targeting::AdPlacementView>>,
    > + Send;
    fn placements(
        &self,
        actor: &Actor,
        offset: i64,
    ) -> impl std::future::Future<
        Output = Result<Vec<crate::advertising::domain::targeting::AdPlacementView>>,
    > + Send;
    fn set_placement_state(
        &self,
        actor: &Actor,
        id: uuid::Uuid,
        input: crate::advertising::domain::targeting::SetPlacementState,
    ) -> impl std::future::Future<
        Output = Result<crate::advertising::domain::targeting::AdPlacementView>,
    > + Send;
}
