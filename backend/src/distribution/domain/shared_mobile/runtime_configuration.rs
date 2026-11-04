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
            "/../api-contract/fixtures/shared-mobile-send-v9.json"
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
