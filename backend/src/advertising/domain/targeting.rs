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

/// A rule has exactly one source scope. Unit identity includes its platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetScope {
    Global {},
    Platform { platform_id: Uuid },
    Unit { platform_id: Uuid, unit_id: Uuid },
}

impl TargetScope {
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

/// A matching rule and its current material version. Matching alone does not
/// authorize use, reserve a rotation slot, or create a delivery decision.
#[derive(Clone, Debug)]
pub struct ScopeMatch {
    pub rule_id: Uuid,
    pub rule_revision: i64,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMaterial {
    pub content: Content,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishMaterial {
    pub expected_revision: i64,
    pub content: Content,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveMaterial {
    pub expected_revision: i64,
    pub reason: String,
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
pub struct CreatePlacementRule {
    pub name: String,
    pub material_id: Uuid,
    pub scope: TargetScope,
    pub enabled: bool,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlacementRuleView {
    pub id: Uuid,
    pub name: String,
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
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let request: CreatePlacementRule =
            serde_json::from_value(fixture["rule_create"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            fixture["rule_create"]
        );
        let placement = PlacementRuleView {
            id: "55555555-5555-4555-8555-555555555555".parse().unwrap(),
            name: request.name,
            material_id: request.material_id,
            scope: request.scope,
            enabled: request.enabled,
            valid_from: request.valid_from,
            valid_until: request.valid_until,
            revision: 1,
        };
        assert_eq!(
            serde_json::to_value(placement).unwrap(),
            fixture["rule_view"]
        );
        let state: SetPlacementState =
            serde_json::from_value(fixture["rule_state"].clone()).unwrap();
        assert_eq!(serde_json::to_value(state).unwrap(), fixture["rule_state"]);
        let mut invalid = fixture["create"].clone();
        invalid["route_id"] = serde_json::json!("not-allowed");
        assert!(serde_json::from_value::<CreateMaterial>(invalid).is_err());
    }
}
