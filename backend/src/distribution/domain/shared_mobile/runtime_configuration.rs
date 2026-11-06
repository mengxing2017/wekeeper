//! Actual execution resources. Development evidence is deliberately not part of this object.
use super::{FlowV3, shared_send_v2::digest};
use crate::business::{Error, Result, require};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfiguration {
    pub id: Uuid,
    pub version: String,
    pub capability: String,
    pub flow: Value,
    pub flow_digest: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ConfigurationIssue {
    pub code: &'static str,
    pub path: &'static str,
}
impl ConfigurationIssue {
    pub fn message(&self) -> &'static str {
        match self.code {
            "configuration_missing" => "平台运行配置未安装，需系统修复",
            "configuration_structure_invalid" => "平台运行配置结构错误，需系统修复",
            "configuration_engine_unsupported" => "平台运行配置与执行引擎不匹配，需系统修复",
            "configuration_constraints_invalid" => "平台运行配置约束不合法，需系统修复",
            "configuration_digest_mismatch" => "平台运行配置摘要不一致，需系统修复",
            _ => "平台运行配置不可用，需系统修复",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeConfigurationStatus {
    pub capability: &'static str,
    pub available: bool,
    pub configuration_id: Option<Uuid>,
    pub version: Option<String>,
    pub flow_digest: Option<String>,
    pub issue: Option<ConfigurationIssue>,
    pub source: &'static str,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}
impl RuntimeConfigurationStatus {
    pub fn project(value: &Value, observed_at: chrono::DateTime<chrono::Utc>) -> Self {
        let issue = inspect_flow(&value["flow"], &value["flow_digest"]).err();
        Self {
            capability: "wecom.group_text",
            available: issue.is_none(),
            configuration_id: value["id"].as_str().and_then(|id| id.parse().ok()),
            version: value["version"].as_str().map(str::to_owned),
            flow_digest: value["flow_digest"].as_str().map(str::to_owned),
            issue,
            source: "platform_runtime_configuration",
            observed_at,
        }
    }
    pub fn matches(&self, expected: &RuntimeConfiguration) -> bool {
        self.available
            && self.configuration_id == Some(expected.id)
            && self.version.as_deref() == Some(expected.version.as_str())
            && self.flow_digest.as_deref() == Some(expected.flow_digest.as_str())
    }
}
pub fn inspect_flow(
    flow: &Value,
    expected_digest: &Value,
) -> std::result::Result<(), ConfigurationIssue> {
    let issue = |code, path| ConfigurationIssue { code, path };
    if flow.is_null() {
        return Err(issue("configuration_missing", "flow"));
    }
    let parsed: FlowV3 = serde_json::from_value(flow.clone())
        .map_err(|_| issue("configuration_structure_invalid", "flow"))?;
    if flow["engine_version"] != "otter-ui/3"
        || flow["protocol_version"] != 9
        || flow["schema_version"] != 3
    {
        return Err(issue(
            "configuration_engine_unsupported",
            "flow.engine_version",
        ));
    }
    parsed
        .validate()
        .map_err(|_| issue("configuration_constraints_invalid", "flow"))?;
    if digest(flow).map_err(|_| issue("configuration_structure_invalid", "flow"))?
        != *expected_digest
    {
        return Err(issue("configuration_digest_mismatch", "flow_digest"));
    }
    Ok(())
}
impl RuntimeConfiguration {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.id.is_nil() && self.capability == "wecom.group_text",
            "configuration_identity_invalid",
        )?;
        super::validate_text(&self.version, 1, 100, 400)?;
        inspect_flow(&self.flow, &Value::String(self.flow_digest.clone()))
            .map_err(|issue| Error::Policy(issue.code))
    }
    pub fn parse(raw: &[u8]) -> Result<Self> {
        require(raw.len() <= 1_100_000, "configuration_too_large")?;
        let value: Self = serde_json::from_slice(raw)
            .map_err(|_| Error::Policy("configuration_package_invalid"))?;
        value.validate()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_reports_absence_without_exposing_rules_and_verify_matches_expected_package() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-configuration-status.json"
        )))
        .unwrap();
        let now = serde_json::from_value(fixture["observed_at"].clone()).unwrap();
        let status = RuntimeConfigurationStatus::project(&Value::Null, now);
        assert_eq!(serde_json::to_value(status).unwrap(), fixture);
        let flow: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/local-group-text-flow.json"
        )))
        .unwrap();
        let mut package = RuntimeConfiguration {
            id: Uuid::new_v4(),
            version: "Synthetic test only".into(),
            capability: "wecom.group_text".into(),
            flow: flow["flow"].clone(),
            flow_digest: flow["flow_digest"].as_str().unwrap().into(),
        };
        let mut value = serde_json::to_value(&package).unwrap();
        let state = RuntimeConfigurationStatus::project(&value, now);
        assert!(state.matches(&package));
        package.id = Uuid::new_v4();
        assert!(!state.matches(&package));
        value["flow_digest"] = serde_json::json!("bad");
        let broken = RuntimeConfigurationStatus::project(&value, now);
        assert!(!broken.available);
        assert!(!broken.matches(&package));
        assert_eq!(broken.issue.unwrap().code, "configuration_digest_mismatch");
    }
    #[test]
    fn screen_runtime_wire_is_shared_with_typescript() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-screen-runtime.json"
        )))
        .unwrap();
        let report: super::super::shared_send_v2::RuntimeReport =
            serde_json::from_value(fixture.clone()).unwrap();
        report.preparation.as_ref().unwrap().validate().unwrap();
        assert_eq!(serde_json::to_value(report).unwrap(), fixture);
    }
    #[test]
    fn configuration_diagnoses_real_failure_without_acceptance_or_artifacts() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/local-group-text-flow.json"
        )))
        .unwrap();
        let config = RuntimeConfiguration {
            id: Uuid::new_v4(),
            version: "Synthetic test only".into(),
            capability: "wecom.group_text".into(),
            flow: fixture["flow"].clone(),
            flow_digest: fixture["flow_digest"].as_str().unwrap().into(),
        };
        RuntimeConfiguration::parse(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(
            inspect_flow(&Value::Null, &Value::Null).unwrap_err().code,
            "configuration_missing"
        );
        assert_eq!(
            inspect_flow(&serde_json::json!({}), &Value::Null)
                .unwrap_err()
                .code,
            "configuration_structure_invalid"
        );
        let mut bad = config.clone();
        bad.flow["engine_version"] = serde_json::json!("other");
        assert_eq!(
            inspect_flow(&bad.flow, &Value::Null).unwrap_err().code,
            "configuration_engine_unsupported"
        );
        bad = config.clone();
        bad.flow["max_targets"] = serde_json::json!(0);
        assert_eq!(
            inspect_flow(&bad.flow, &Value::Null).unwrap_err().code,
            "configuration_constraints_invalid"
        );
        assert_eq!(
            inspect_flow(&config.flow, &Value::Null).unwrap_err().code,
            "configuration_digest_mismatch"
        );
        let mut package = serde_json::to_value(config).unwrap();
        package["acceptance_id"] = serde_json::json!("not a runtime field");
        assert!(RuntimeConfiguration::parse(&serde_json::to_vec(&package).unwrap()).is_err());
    }
}
