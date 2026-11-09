//! Result provenance is independent of task scheduling and authorization.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: i64,
    pub channel: String,
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub target_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
    pub batch_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub invocation_id: Option<Uuid>,
    pub evidence_type: String,
    pub observation_source: String,
    pub observed_at: DateTime<Utc>,
    pub device_occurred_at: Option<DateTime<Utc>>,
    pub external_status: Option<String>,
    pub normalized_result: String,
    pub detail: Value,
    pub reconstructed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_contract_is_serialized_by_rust() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/channel-execution-observation.json"
        ))
        .unwrap();
        let row: Observation = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(row).unwrap(), fixture);
    }
}
