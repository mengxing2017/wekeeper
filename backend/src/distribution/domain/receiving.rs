//! Unit-owned channel configuration. Enabling grants automatic handling of new messages.
use crate::business::{Result, require, text};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Mobile,
    Wecom,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Wecom => "wecom",
        }
    }
    pub fn kind(self) -> &'static str {
        match self {
            Self::Mobile => "mobile-receiving-configurations",
            Self::Wecom => "wecom-receiving-configurations",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    All,
    RoundRobin,
}

/// Rotate a stable ordered resource list without relying on database row order.
pub fn rotate(ids: &[Uuid], next: Option<Uuid>) -> Vec<Uuid> {
    let start = next
        .and_then(|id| ids.iter().position(|v| *v == id))
        .unwrap_or(0);
    ids.iter()
        .cycle()
        .skip(start)
        .take(ids.len())
        .copied()
        .collect()
}
pub fn successor(ids: &[Uuid], selected: Uuid) -> Option<Uuid> {
    ids.iter()
        .position(|v| *v == selected)
        .map(|i| ids[(i + 1) % ids.len()])
}
pub fn reconcile_cursor(old: &[Uuid], new: &[Uuid], next: Option<Uuid>) -> Option<Uuid> {
    rotate(old, next)
        .into_iter()
        .find(|id| new.contains(id))
        .or_else(|| new.first().copied())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<super::shared_mobile::MobileClient>,
    pub name: String,
    pub enabled: bool,
    pub target_ids: Vec<Uuid>,
    /// Explicit preference order, scoped to this configuration.
    pub device_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
    pub target_selection_mode: SelectionMode,
    pub device_dispatch_mode: Option<SelectionMode>,
}

impl ConfigurationInput {
    pub fn validate(&mut self, channel: Channel) -> Result<()> {
        self.name = text(&self.name, 1, 120)?;
        require(
            (channel == Channel::Mobile) == self.client_kind.is_some(),
            "receiving_client_required",
        )?;
        require(
            match channel {
                Channel::Mobile => self.device_dispatch_mode.is_some(),
                Channel::Wecom => self.device_dispatch_mode.is_none(),
            },
            "手机下发方式与通道不匹配",
        )?;
        for ids in [&self.target_ids, &self.device_ids, &self.sender_ids] {
            require(
                ids.len() <= 1000
                    && ids.iter().all(|id| !id.is_nil())
                    && ids.iter().collect::<HashSet<_>>().len() == ids.len(),
                "接收配置资源必须唯一且有效，最多 1000 项",
            )?;
        }
        require(
            match channel {
                Channel::Mobile => self.sender_ids.is_empty(),
                Channel::Wecom => self.device_ids.is_empty(),
            },
            "接收配置不能包含其他通道的资源",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<super::shared_mobile::MobileClient>,
    pub id: Uuid,
    pub business_unit_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub target_ids: Vec<Uuid>,
    pub device_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
    pub target_selection_mode: SelectionMode,
    pub device_dispatch_mode: Option<SelectionMode>,
    pub revision: i64,
    pub next_target_id: Option<Uuid>,
    pub next_device_id: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<super::shared_mobile::MobileClient>,
    pub name: String,
    pub directory_group_id: Option<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetResource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<super::shared_mobile::MobileClient>,
    pub id: Uuid,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configurations_reject_cross_channel_and_routing_facts() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/receiving-configuration.json"
        ))
        .unwrap();
        let target_input: TargetInput =
            serde_json::from_value(fixture["target_input"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(target_input).unwrap(),
            fixture["target_input"]
        );
        let target: TargetResource = serde_json::from_value(fixture["target"].clone()).unwrap();
        assert_eq!(serde_json::to_value(target).unwrap(), fixture["target"]);
        let mut input: ConfigurationInput =
            serde_json::from_value(fixture["input"].clone()).unwrap();
        input.validate(Channel::Mobile).unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap(), fixture["input"]);
        let config: Configuration =
            serde_json::from_value(fixture["configuration"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            fixture["configuration"]
        );
        let mut official: ConfigurationInput =
            serde_json::from_value(fixture["wecom_input"].clone()).unwrap();
        official.validate(Channel::Wecom).unwrap();
        assert_eq!(
            serde_json::to_value(&official).unwrap(),
            fixture["wecom_input"]
        );
        assert!(official.validate(Channel::Mobile).is_err());
        let official: Configuration =
            serde_json::from_value(fixture["wecom_configuration"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(official).unwrap(),
            fixture["wecom_configuration"]
        );
        assert!(input.validate(Channel::Wecom).is_err());
        input.device_ids.push(input.device_ids[0]);
        assert!(input.validate(Channel::Mobile).is_err());
        for key in ["route_id", "advertisement_id", "authorization_ref"] {
            let mut invalid = fixture["input"].clone();
            invalid[key] = serde_json::json!(Uuid::new_v4());
            assert!(serde_json::from_value::<ConfigurationInput>(invalid).is_err());
        }
    }
}

/// Frozen, channel-owned execution input; no source-route or business version dependency.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeliveryPlan {
    pub id: Uuid,
    pub configuration_id: Uuid,

    pub channel: Channel,
    pub authorized_by: Uuid,
    pub targets: Vec<DeliveryTarget>,
    pub device_dispatch_mode: Option<SelectionMode>,
    pub device_ids: Vec<Uuid>,
    pub delivery_id: Option<Uuid>,
    pub not_before: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeliveryTarget {
    pub id: Uuid,
    pub name: String,
    pub identity: serde_json::Value,
    pub resource_available: bool,
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    #[test]
    fn rotation_and_edit_preserve_next_identity() {
        let ids: Vec<_> = (1..=3).map(Uuid::from_u128).collect();
        assert_eq!(rotate(&ids, Some(ids[1])), vec![ids[1], ids[2], ids[0]]);
        assert_eq!(successor(&ids, ids[2]), Some(ids[0]));
        assert_eq!(
            reconcile_cursor(&ids, &[ids[0], ids[2]], Some(ids[1])),
            Some(ids[2])
        );
        assert_eq!(reconcile_cursor(&ids, &[], Some(ids[1])), None);
    }
}

/// Deterministic ownership; availability here is business authorization, never presence.
pub fn allocate_targets(
    inputs: &[(Channel, SelectionMode, Vec<DeliveryTarget>)],
) -> Vec<Vec<DeliveryTarget>> {
    let mut assigned = HashSet::new();
    let mut selected = vec![vec![]; inputs.len()];
    for available in [true, false] {
        for (i, (channel, mode, targets)) in inputs.iter().enumerate() {
            for target in targets {
                if *mode == SelectionMode::RoundRobin && !selected[i].is_empty() {
                    break;
                }
                if target.resource_available != available
                    || !assigned.insert((channel.as_str(), target.id))
                {
                    continue;
                }
                selected[i].push(target.clone());
            }
        }
    }
    selected
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhoneSummary {
    pub target_count: i64,
    pub expected_count: i64,
    pub materialized_count: i64,
    pub sent: i64,
    pub failed: i64,
    pub unknown: i64,
    pub pending: i64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhoneDelivery {
    pub last_progress_at: chrono::DateTime<chrono::Utc>,
    pub id: Uuid,
    pub dispatch_target_id: Uuid,
    pub device_id: Option<Uuid>,
    pub position: i32,
    pub message_id: Uuid,
    pub target_id: Uuid,
    pub target_name: String,
    pub device_name: Option<String>,
    pub intake_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub item_id: Option<Uuid>,
    pub outcome: String,
    pub stage: String,
    pub reason_code: String,
    pub historical_risk: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhoneTarget {
    pub last_progress_at: Option<chrono::DateTime<chrono::Utc>>,
    pub historical_risk: Option<bool>,
    pub id: Uuid,
    pub message_id: Uuid,
    pub channel: String,
    pub target_id: Uuid,
    pub target_name: String,
    pub expected_count: i32,
    pub materialized_count: i32,
    pub stopped: bool,
    pub sent: i64,
    pub failed: i64,
    pub unknown: i64,
    pub pending: i64,
    pub outcome: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhoneProgress {
    pub id: Uuid,
    pub items: Vec<PhoneDelivery>,
    pub targets: Vec<PhoneTarget>,
    pub summary: PhoneSummary,
    pub has_more: bool,
}

#[cfg(test)]
mod policy_contract_tests {
    use super::*;
    #[test]
    fn management_phone_progress_matches_shared_fixture() {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/receiving-phone-progress.json"
        ))
        .unwrap();
        let view: PhoneProgress = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(view).unwrap(), value);
    }
    #[test]
    fn invalid_mode_and_wrong_channel_strategy_are_rejected() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/receiving-configuration.json"
        ))
        .unwrap();
        let mut input = fixture["input"].clone();
        input["target_selection_mode"] = serde_json::json!("random");
        assert!(serde_json::from_value::<ConfigurationInput>(input).is_err());
        let mut input: ConfigurationInput =
            serde_json::from_value(fixture["input"].clone()).unwrap();
        input.device_ids.clear();
        assert!(input.validate(Channel::Wecom).is_err());
    }
    #[test]
    fn configuration_requires_explicit_current_strategy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/receiving-configuration.json"
        ))
        .unwrap();
        let mut old_input = fixture["input"].clone();
        old_input
            .as_object_mut()
            .unwrap()
            .remove("target_selection_mode");
        old_input
            .as_object_mut()
            .unwrap()
            .remove("device_dispatch_mode");
        assert!(serde_json::from_value::<ConfigurationInput>(old_input).is_err());

        let mut missing_phone_mode = fixture["input"].clone();
        missing_phone_mode
            .as_object_mut()
            .unwrap()
            .remove("device_dispatch_mode");
        let mut input: ConfigurationInput = serde_json::from_value(missing_phone_mode).unwrap();
        assert!(input.validate(Channel::Mobile).is_err());
    }
    #[test]
    fn shared_group_ownership_prioritizes_authorized_resource_and_respects_rotation_limit() {
        let target = |id, available| DeliveryTarget {
            id: Uuid::from_u128(id),
            name: "群".into(),
            identity: serde_json::json!({}),
            resource_available: available,
        };
        let inputs = vec![
            (
                Channel::Mobile,
                SelectionMode::RoundRobin,
                vec![target(1, false), target(2, true)],
            ),
            (
                Channel::Mobile,
                SelectionMode::All,
                vec![target(1, true), target(2, true)],
            ),
        ];
        let result = allocate_targets(&inputs);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0].id, Uuid::from_u128(2));
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[1][0].id, Uuid::from_u128(1));
    }
}
