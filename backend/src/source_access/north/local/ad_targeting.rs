//! Source-owned identity resolution for advertising discovery and frozen decisions.
use crate::advertising::{
    domain::{
        AdvertisingError,
        targeting::{ScopeMatchPage, SourceIdentity, TargetScope},
    },
    north::local::MatchingAds,
    south::port::AdPlacementReader,
};
use crate::source_access::{domain::Result, south::port::AcceptedIdentityStore};
use uuid::Uuid;

#[derive(Debug)]
pub enum DiscoveryError {
    Identity(crate::source_access::domain::Fault),
    Advertising(AdvertisingError),
}

/// Connects accepted identity to read-only advertising discovery. This does
/// not authorize a material, select a winner, or create a delivery decision.
pub struct CandidateDiscovery<I, R> {
    pub identities: I,
    pub advertisements: MatchingAds<R>,
}

impl<I: AcceptedIdentityStore, R: AdPlacementReader> CandidateDiscovery<I, R> {
    pub async fn page(
        &self,
        platform_id: Uuid,
        receipt_id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
        after: Option<Uuid>,
    ) -> std::result::Result<ScopeMatchPage, DiscoveryError> {
        let identity = self
            .identities
            .find_identity(platform_id, receipt_id)
            .await
            .map_err(DiscoveryError::Identity)?;
        self.advertisements
            .page(
                SourceIdentity {
                    platform_id: identity.platform_id,
                    unit_id: identity.unit_id,
                },
                at,
                after,
            )
            .await
            .map_err(DiscoveryError::Advertising)
    }
}

pub struct ScopeMatcher<S>(pub S);

impl<S: AcceptedIdentityStore> ScopeMatcher<S> {
    pub async fn matching_indices(
        &self,
        platform_id: Uuid,
        receipt_id: Uuid,
        scopes: &[TargetScope],
    ) -> Result<Vec<usize>> {
        let identity = self.0.find_identity(platform_id, receipt_id).await?;
        let source = SourceIdentity {
            platform_id: identity.platform_id,
            unit_id: identity.unit_id,
        };
        Ok(scopes
            .iter()
            .enumerate()
            .filter_map(|(index, scope)| scope.matches(source).then_some(index))
            .collect())
    }
}
