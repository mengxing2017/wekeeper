//! Routing rules are independent of HTTP and persistence.
use super::{Fault, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Wecom,
    Mobile,
}
impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wecom => "wecom",
            Self::Mobile => "mobile",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub active: bool,
    pub revision: Option<i64>,
    pub unit_id: Option<Uuid>,
}
impl EntityInput {
    pub fn validate(&self) -> Result<()> {
        ensure(
            (1..=120).contains(&self.name.trim().chars().count())
                && self.description.chars().count() <= 2000,
            "invalid_request",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionConfig {
    pub channel: Channel,
    pub workspace_id: Uuid,
    pub target_ids: Vec<Uuid>,
}
impl ExecutionConfig {
    pub fn normalize(&mut self) -> Result<()> {
        ensure(
            self.target_ids.len()
                <= match self.channel {
                    Channel::Mobile => 100,
                    Channel::Wecom => 1000,
                }
                && self.target_ids.iter().all(|id| !id.is_nil()),
            "invalid_request",
        )?;
        self.target_ids.sort();
        self.target_ids.dedup();
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationInput {
    pub revision: i64,
    pub unit_id: Uuid,
    pub group_id: Uuid,
    pub execution: Option<ExecutionConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleInput {
    pub revision: i64,
    pub action: LifecycleAction,
    pub reason: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Enable,
    Pause,
    Resume,
    Retire,
}
impl LifecycleInput {
    pub fn validate(&self) -> Result<()> {
        ensure(
            !self.reason.trim().is_empty() && self.reason.chars().count() <= 2000,
            "invalid_request",
        )
    }
    pub fn next_state(&self, current: &str, retired: bool, ready: bool) -> Result<&'static str> {
        ensure(!retired, "route_retired")?;
        match self.action {
            LifecycleAction::Enable | LifecycleAction::Resume => {
                ensure(ready, "route_configuration_required")?;
                Ok("running")
            }
            LifecycleAction::Pause => {
                ensure(current != "draft", "invalid_transition")?;
                Ok("paused")
            }
            LifecycleAction::Retire => Ok("paused"),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerInput {
    pub revision: i64,
    pub username: String,
    pub granted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTargetInput {
    pub revision: i64,
    pub execution: ExecutionConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TargetSnapshot {
    pub id: Uuid,
    pub name: String,
    pub identity: serde_json::Value,
    pub resource_available: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSnapshot {
    pub route_id: String,
    pub revision: i64,
    pub source_id: Uuid,
    pub unit_id: Uuid,
    pub unit_name: String,
    pub group_id: Uuid,
    pub group_name: String,
    pub execution: Option<ExecutionConfig>,
    pub targets: Vec<TargetSnapshot>,
    pub configured_by: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub ready: bool,
    pub issues: Vec<String>,
}
impl CheckResult {
    pub fn new(issues: Vec<String>) -> Self {
        Self {
            ready: issues.is_empty(),
            issues,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Resolution {
    Ready { snapshot: RouteSnapshot },
    Waiting { reason_code: String },
}
impl From<serde_json::Error> for Fault {
    fn from(_: serde_json::Error) -> Self {
        Self("invalid_request")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_identity_is_not_its_display_name() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut c = ExecutionConfig {
            channel: Channel::Mobile,
            workspace_id: Uuid::new_v4(),
            target_ids: vec![a, b, a],
        };
        c.normalize().unwrap();
        assert_eq!(c.target_ids.len(), 2);
    }
    #[test]
    fn retirement_and_missing_config_cannot_be_resumed() {
        let c = LifecycleInput {
            revision: 1,
            action: LifecycleAction::Resume,
            reason: "恢复".into(),
        };
        assert_eq!(
            c.next_state("paused", true, true).unwrap_err().0,
            "route_retired"
        );
        assert_eq!(
            c.next_state("paused", false, false).unwrap_err().0,
            "route_configuration_required"
        );
    }
    #[test]
    fn configuration_rejects_platform_aliases_and_mixed_channels() {
        let config = serde_json::json!({"channel":"mobile","workspace_id":Uuid::nil(),"target_ids":[],"wecom_workspace_id":Uuid::nil()});
        assert!(serde_json::from_value::<ExecutionConfig>(config).is_err());
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    Assigned,
    Entities {
        kind: String,
    },
    Entity {
        kind: String,
        id: Option<Uuid>,
        input: EntityInput,
    },
    Configuration {
        route: String,
    },
    Configure {
        route: String,
        input: ConfigurationInput,
    },
    Check {
        route: String,
    },
    Lifecycle {
        route: String,
        input: LifecycleInput,
    },
    Revisions {
        route: String,
        page: i64,
    },
    Managers {
        route: String,
    },
    Manager {
        route: String,
        input: ManagerInput,
    },
    Resources {
        channel: Channel,
        workspace: Option<Uuid>,
    },
    GroupTargets {
        group: Uuid,
    },
    SetGroupTargets {
        group: Uuid,
        input: GroupTargetInput,
    },
    Handoff {
        route: String,
    },
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn shared_fixture_roundtrips_rust_fields_and_checks() {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-routing.json"
        ))
        .unwrap();
        let input: ConfigurationInput = serde_json::from_value(v["input"].clone()).unwrap();
        assert_eq!(serde_json::to_value(input).unwrap(), v["input"]);
        let snapshot: RouteSnapshot = serde_json::from_value(v["snapshot"].clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), v["snapshot"]);
        let check: CheckResult = serde_json::from_value(v["check"].clone()).unwrap();
        assert_eq!(serde_json::to_value(check).unwrap(), v["check"]);
    }
}

/// Machine-visible metadata deliberately excludes internal organization and resources.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCapability {
    pub route_id: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_state: Option<String>,
}
#[cfg(test)]
mod capability_contract_tests {
    use super::*;
    #[test]
    fn machine_route_metadata_matches_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-authentication-v2.json"
        ))
        .unwrap();
        let route = RouteCapability {
            route_id: "fixture-route".into(),
            available: true,
            display_name: Some("通知路由".into()),
            execution_state: Some("paused".into()),
        };
        assert_eq!(
            serde_json::to_value(route).unwrap(),
            fixture["data"]["attributes"]["routes"][0]
        );
        let unavailable = RouteCapability {
            route_id: "unknown".into(),
            available: false,
            display_name: None,
            execution_state: None,
        };
        assert_eq!(
            serde_json::to_value(unavailable).unwrap(),
            serde_json::json!({"route_id":"unknown","available":false})
        );
    }
}
