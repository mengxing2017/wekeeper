//! Source identity and targeting are independent of message routing.
use super::Content;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Authenticated business identity resolved by the caller before advertising.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceIdentity {
    pub platform_id: Uuid,
    pub unit_id: Uuid,
}

/// An advertisement is bound directly to a source scope. Unit identity includes its platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetScope {
    Global {},
    Platform { platform_id: Uuid },
    Unit { platform_id: Uuid, unit_id: Uuid },
}

impl TargetScope {
    /// Business precedence: unit > platform > global. Selection within this scope chooses one advertisement.
    fn priority(self) -> u8 {
        match self {
            Self::Global {} => 0,
            Self::Platform { .. } => 1,
            Self::Unit { .. } => 2,
        }
    }

    /// Callers must independently verify that a unit belongs to its platform.
    pub fn matches(self, source: SourceIdentity) -> bool {
        match self {
            Self::Global {} => true,
            Self::Platform { platform_id } => platform_id == source.platform_id,
            Self::Unit {
                platform_id,
                unit_id,
            } => platform_id == source.platform_id && unit_id == source.unit_id,
        }
    }
}

/// A matching advertisement placement and its current material version. Matching alone does not
/// authorize use, reserve a rotation slot, or create a delivery decision.
#[derive(Clone, Debug)]
pub struct ScopeMatch {
    pub placement_id: Uuid,
    pub placement_revision: i64,
    pub scope: TargetScope,
    pub material_id: Uuid,
    pub material_version_id: Uuid,
    pub content: Content,
}

#[derive(Clone, Debug)]
pub struct ScopeMatchPage {
    pub items: Vec<ScopeMatch>,
    pub next_after: Option<Uuid>,
}

/// Keep every candidate at the highest matching scope. Same-scope selection,
/// composition and delivery authorization belong to the later decision use case.
pub fn preferred_candidates(
    source: SourceIdentity,
    mut candidates: Vec<ScopeMatch>,
) -> Vec<ScopeMatch> {
    candidates.retain(|candidate| candidate.scope.matches(source));
    let priority = candidates.iter().map(|item| item.scope.priority()).max();
    candidates.retain(|candidate| Some(candidate.scope.priority()) == priority);
    candidates
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMaterial {
    pub content: Content,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishMaterial {
    pub expected_revision: i64,
    pub content: Content,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveMaterial {
    pub expected_revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MaterialView {
    pub id: Uuid,
    pub revision: i64,
    pub archived: bool,
    pub content_version: Uuid,
    pub content: Content,
}

#[derive(Clone, Debug, Serialize)]
pub struct MaterialVersionView {
    pub id: Uuid,
    pub material_id: Uuid,
    pub version: i64,
    pub content: Content,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManagementPage<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAdPlacement {
    pub material_id: Uuid,
    pub scope: TargetScope,
    pub enabled: bool,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdPlacementView {
    pub id: Uuid,
    pub material_id: Uuid,
    pub scope: TargetScope,
    pub enabled: bool,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPlacementState {
    pub expected_revision: i64,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_ignores_foreign_scopes_and_preserves_same_scope_candidates() {
        let source = SourceIdentity {
            platform_id: Uuid::new_v4(),
            unit_id: Uuid::new_v4(),
        };
        let candidate = |scope| ScopeMatch {
            placement_id: Uuid::new_v4(),
            placement_revision: 1,
            scope,
            material_id: Uuid::new_v4(),
            material_version_id: Uuid::new_v4(),
            content: Content {
                name: "ad".into(),
                body: "body".into(),
                link: None,
            },
        };
        let platform = candidate(TargetScope::Platform {
            platform_id: source.platform_id,
        });
        let foreign_unit = candidate(TargetScope::Unit {
            platform_id: Uuid::new_v4(),
            unit_id: source.unit_id,
        });
        let candidates = preferred_candidates(
            source,
            vec![
                candidate(TargetScope::Global {}),
                foreign_unit.clone(),
                platform.clone(),
            ],
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].placement_id, platform.placement_id);
        let unit = candidate(TargetScope::Unit {
            platform_id: source.platform_id,
            unit_id: source.unit_id,
        });
        let tied = candidate(unit.scope);
        let candidates = preferred_candidates(source, vec![platform, unit.clone(), tied.clone()]);
        assert_eq!(
            candidates
                .iter()
                .map(|item| item.placement_id)
                .collect::<Vec<_>>(),
            vec![unit.placement_id, tied.placement_id]
        );
        assert!(preferred_candidates(source, vec![foreign_unit]).is_empty());
        assert!(preferred_candidates(source, Vec::new()).is_empty());
    }

    #[test]
    fn scopes_match_business_identity_without_crossing_platforms() {
        let platform_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        let source = SourceIdentity {
            platform_id,
            unit_id,
        };
        assert!(TargetScope::Global {}.matches(source));
        assert!(TargetScope::Platform { platform_id }.matches(source));
        assert!(
            TargetScope::Unit {
                platform_id,
                unit_id
            }
            .matches(source)
        );
        assert!(
            !TargetScope::Unit {
                platform_id: Uuid::new_v4(),
                unit_id,
            }
            .matches(source)
        );
        assert!(
            !TargetScope::Unit {
                platform_id,
                unit_id: Uuid::new_v4(),
            }
            .matches(source)
        );
    }

    #[test]
    fn serialized_scope_rejects_routing_and_invalid_shapes() {
        for value in [
            serde_json::json!({"kind":"route", "id":"r"}),
            serde_json::json!({"kind":"global", "unit_id":Uuid::new_v4()}),
            serde_json::json!({"kind":"unit", "unit_id":Uuid::new_v4()}),
        ] {
            assert!(serde_json::from_value::<TargetScope>(value).is_err());
        }
    }

    #[test]
    fn material_types_match_shared_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/scoped-ad-material.json"
        ))
        .unwrap();
        let create: CreateMaterial = serde_json::from_value(fixture["create"].clone()).unwrap();
        let publish: PublishMaterial = serde_json::from_value(fixture["publish"].clone()).unwrap();
        assert_eq!(serde_json::to_value(&create).unwrap(), fixture["create"]);
        assert_eq!(serde_json::to_value(&publish).unwrap(), fixture["publish"]);
        let archive: ArchiveMaterial = serde_json::from_value(fixture["archive"].clone()).unwrap();
        assert_eq!(serde_json::to_value(archive).unwrap(), fixture["archive"]);
        let view = MaterialView {
            id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
            revision: 1,
            archived: false,
            content_version: "22222222-2222-4222-8222-222222222222".parse().unwrap(),
            content: create.content,
        };
        assert_eq!(serde_json::to_value(view).unwrap(), fixture["material"]);
        let version = MaterialVersionView {
            id: fixture["material_version"]["id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            material_id: fixture["material_version"]["material_id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            version: 1,
            content: serde_json::from_value(fixture["material_version"]["content"].clone())
                .unwrap(),
            created_by: fixture["material_version"]["created_by"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
        };
        assert_eq!(
            serde_json::to_value(version).unwrap(),
            fixture["material_version"]
        );
        let request: CreateAdPlacement =
            serde_json::from_value(fixture["placement_create"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            fixture["placement_create"]
        );
        let placement = AdPlacementView {
            id: "55555555-5555-4555-8555-555555555555".parse().unwrap(),
            material_id: request.material_id,
            scope: request.scope,
            enabled: request.enabled,
            valid_from: request.valid_from,
            valid_until: request.valid_until,
            revision: 1,
        };
        assert_eq!(
            serde_json::to_value(placement).unwrap(),
            fixture["placement_view"]
        );
        let state: SetPlacementState =
            serde_json::from_value(fixture["placement_state"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            fixture["placement_state"]
        );
        // Daily advertising operations have no operator-supplied reason field.
        // Reject the retired field rather than retaining a compatibility branch.
        for key in [
            "create",
            "publish",
            "archive",
            "placement_create",
            "placement_state",
        ] {
            let mut retired = fixture[key].clone();
            retired["reason"] = serde_json::json!("obsolete");
            let rejected = match key {
                "create" => serde_json::from_value::<CreateMaterial>(retired).is_err(),
                "publish" => serde_json::from_value::<PublishMaterial>(retired).is_err(),
                "archive" => serde_json::from_value::<ArchiveMaterial>(retired).is_err(),
                "placement_create" => serde_json::from_value::<CreateAdPlacement>(retired).is_err(),
                _ => serde_json::from_value::<SetPlacementState>(retired).is_err(),
            };
            assert!(rejected, "retired reason accepted by {key}");
        }
        let mut invalid = fixture["create"].clone();
        invalid["route_id"] = serde_json::json!("not-allowed");
        assert!(serde_json::from_value::<CreateMaterial>(invalid).is_err());
    }
}
