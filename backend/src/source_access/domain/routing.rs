//! Intake route access is separate from channel receiving configuration.
use super::Fault;
use serde::{Deserialize, Serialize};
impl From<serde_json::Error> for Fault {
    fn from(_: serde_json::Error) -> Self {
        Self("invalid_request")
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerInput {
    pub username: String,
    pub granted: bool,
}
#[derive(Clone, Debug)]
pub enum Command {
    Assigned,
    Configuration { route: String },
    Managers { route: String },
    Manager { route: String, input: ManagerInput },
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
