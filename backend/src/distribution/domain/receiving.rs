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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationInput {
    pub name: String,
    pub enabled: bool,
    pub target_ids: Vec<Uuid>,
    /// Explicit preference order, scoped to this configuration.
    pub device_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
}

impl ConfigurationInput {
    pub fn validate(&mut self, channel: Channel) -> Result<()> {
        self.name = text(&self.name, 1, 120)?;
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
    pub id: Uuid,
    pub business_unit_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub target_ids: Vec<Uuid>,
    pub device_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetInput {
    pub name: String,
    pub directory_group_id: Option<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetResource {
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
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeliveryTarget {
    pub id: Uuid,
    pub name: String,
    pub identity: serde_json::Value,
    pub resource_available: bool,
}
