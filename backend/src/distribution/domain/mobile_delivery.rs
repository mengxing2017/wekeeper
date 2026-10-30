//! Phone execution contains business data only; no script, coordinate or platform protocol.
use crate::business::{Error, Result, hash, require, text};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const VISIBLE_PROTOCOL_VERSION: u32 = 2;

mod visible;
pub use visible::*;
mod configuration;
pub use configuration::*;
pub const ACCOUNT_INTERVAL_SECONDS: i64 = 1800;
pub const CONTENT_INTERVAL_SECONDS: i64 = 600;
pub const CLAIM_SECONDS: i64 = 120;
pub const PERMIT_SECONDS: i64 = 60;
pub fn normalize(content: &str) -> Result<String> {
    let content: String = content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .nfc()
        .collect();
    require(
        !content.is_empty()
            && content.len() <= 4000
            && !content
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "正文必须为 1–4000 UTF-8 字节纯文本",
    )?;
    Ok(content)
}
pub fn digest(content: &str) -> Result<String> {
    Ok(hash(&normalize(content)?))
}
pub fn policy(limit: i32, account: bool) -> Result<()> {
    require(
        (0..=if account { 50 } else { 30 }).contains(&limit),
        "额度超出允许范围",
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentSource {
    Text {
        text_content: String,
    },
    Posts {
        template_version_id: Uuid,
        post_ids: Vec<Uuid>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInput {
    pub group_id: Uuid,
    pub sender_account_id: Uuid,
    pub source: ContentSource,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub expected_revision: Option<i64>,
    pub activity_id: String,
    pub items: Vec<ItemInput>,
}
impl Draft {
    pub fn validate(&self) -> Result<()> {
        text(&self.activity_id, 1, 200)?;
        require(
            !self.items.is_empty() && self.items.len() <= 100,
            "任务需要 1–100 个群",
        )?;
        let mut groups = std::collections::BTreeSet::new();
        for item in &self.items {
            require(groups.insert(item.group_id), "同任务目标群不可重复")?;
            match &item.source {
                ContentSource::Text { text_content } => {
                    normalize(text_content)?;
                }
                ContentSource::Posts { post_ids, .. } => {
                    let unique: std::collections::BTreeSet<_> = post_ids.iter().collect();
                    require(
                        !post_ids.is_empty()
                            && post_ids.len() <= 20
                            && unique.len() == post_ids.len(),
                        "请选择 1–20 条不同帖子",
                    )?;
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    CreateDevice {
        name: String,
    },
    UpdateDevice {
        expected_revision: i64,
        enabled: bool,
        reason: String,
    },
    Enrollment {
        expected_revision: i64,
    },
    Revoke {
        expected_revision: i64,
        reason: String,
    },
    CreateAccount {
        client_kind: String,
        #[serde(default)]
        identity_namespace: Option<String>,
        #[serde(default)]
        external_key: Option<String>,
        responsible_actor_id: Uuid,
        identity_evidence_ref: String,
        #[serde(default, skip_serializing_if = "IdentityMode::is_legacy")]
        identity_mode: IdentityMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible_profile: Option<AccountProfile>,
    },
    UpdateAccountProfile {
        expected_revision: i64,
        visible_profile: AccountProfile,
        reason: String,
    },
    ReviewAccountProfile {
        expected_revision: i64,
        evidence_ref: String,
    },
    UpdateGroupProfile {
        expected_revision: i64,
        name: String,
        visible_profile: GroupProfile,
        reason: String,
    },
    ReviewGroupProfile {
        expected_revision: i64,
        evidence_ref: String,
    },
    CreateStrategy {
        #[serde(flatten)]
        input: StrategyInput,
    },
    UpdateStrategy {
        expected_revision: i64,
        #[serde(flatten)]
        input: StrategyInput,
    },
    RunStrategy {
        expected_revision: i64,
    },
    CreateConfiguration {
        input: Box<ConfigurationInput>,
    },
    PublishConfiguration {
        expected_revision: i64,
        school_id: Uuid,
        device_id: Option<Uuid>,
        evidence_ref: String,
    },
    RevokeConfigurationRelease {
        expected_revision: i64,
        reason: String,
    },
    ResolveConfiguration {
        package_name: String,
        version_name: String,
        version_code: i64,
        engine_version: String,
        capabilities: Vec<String>,
    },
    PreviewRunRevision {
        configuration_release_id: Uuid,
    },
    ReviseRun {
        expected_revision: i64,
        configuration_release_id: Uuid,
        preview_digest: String,
        evidence_ref: String,
    },
    BindDevice {
        expected_revision: i64,
        sender_account_id: Uuid,
        identity_evidence_ref: String,
    },
    CreateGroup {
        name: String,
        client_kind: String,
        #[serde(default)]
        identity_namespace: Option<String>,
        #[serde(default)]
        external_key: Option<String>,
        identity_evidence_ref: String,
        wecom_group_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "IdentityMode::is_legacy")]
        identity_mode: IdentityMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible_profile: Option<GroupProfile>,
    },
    BindGroup {
        sender_account_id: Uuid,
        mapping_revision: i64,
        reason: String,
    },
    AccountPolicy {
        expected_revision: i64,
        daily_limit: i32,
        reason: String,
    },
    GroupPolicy {
        expected_revision: i64,
        daily_limit: i32,
        reason: String,
    },
    Settings {
        expected_revision: i64,
        enabled: bool,
        history_reviewed: bool,
        approved_client_version: String,
        approved_script_version: String,
        approved_baseline_version: String,
        evidence_ref: String,
    },
    Draft {
        #[serde(flatten)]
        draft: Draft,
    },
    Preview {
        expected_revision: i64,
    },
    Confirm {
        expected_revision: i64,
        preview_digest: String,
        content_review: bool,
        scope_acceptance: bool,
        substantive_difference_reviewed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shared_text_scope_acceptance: Option<bool>,
        evidence_ref: String,
    },
    Stop {
        expected_revision: i64,
        reason: String,
    },
    Reconcile {
        expected_revision: i64,
        decision: String,
        evidence_ref: String,
        reason: String,
    },
    Resend {
        expected_revision: i64,
        reason: String,
    },
    Enroll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device_id: Option<Uuid>,
        code: String,
        client_instance_id: String,
        protocol_version: u32,
    },
    Rotate {},
    Heartbeat {
        binding_revision: i64,
        sender_account_id: Uuid,
        client_version: String,
        script_version: String,
        baseline_version: String,
        permissions_ready: bool,
        current_claim_id: Option<Uuid>,
    },
    Claim {
        binding_revision: i64,
    },
    Renew {
        fence: i64,
        binding_revision: i64,
    },
    Prepare {
        fence: i64,
        snapshot_digest: String,
        observed_account_id: Uuid,
        observed_group_id: Uuid,
        client_version: String,
        script_version: String,
        baseline_version: String,
        checks_passed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verification: Option<VisibleVerification>,
    },
    Permit {
        fence: i64,
        snapshot_digest: String,
        preparation_id: Uuid,
    },
    Result {
        invocation_id: Uuid,
        permit_token: String,
        event_id: Uuid,
        result: String,
        evidence_ref: String,
        observed_account_id: Uuid,
        observed_group_id: Uuid,
        content_digest: String,
        local_state: String,
        reported_clicked_at: Option<chrono::DateTime<chrono::Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verification: Option<VisibleVerification>,
    },
    Abort {
        fence: i64,
        reason: String,
    },
}
#[derive(Clone, Debug)]
pub enum Principal {
    Actor(Uuid),
    Device { id: Uuid, generation: i64 },
    Enrollment,
    DeviceV2 { id: Uuid, generation: i64 },
    EnrollmentV2,
    DeviceV3 { id: Uuid, generation: i64 },
    EnrollmentV3,
}
impl Principal {
    pub fn key(&self) -> String {
        match self {
            Self::Actor(id) => format!("actor:{id}"),
            Self::Device { id, generation } => format!("device:{id}:{generation}"),
            Self::Enrollment => "enrollment".into(),
            Self::EnrollmentV2 => "enrollment:v2".into(),
            Self::EnrollmentV3 => "enrollment:v3".into(),
            Self::DeviceV3 { id, generation } => format!("device:{id}:{generation}:v3"),
            Self::DeviceV2 { id, generation } => format!("device:{id}:{generation}:v2"),
        }
    }
    pub fn protocol(&self) -> u32 {
        if matches!(self, Self::DeviceV3 { .. } | Self::EnrollmentV3) {
            3
        } else if matches!(self, Self::DeviceV2 { .. } | Self::EnrollmentV2) {
            2
        } else {
            1
        }
    }
    pub fn for_protocol(self, version: u32) -> Self {
        match self {
            Self::Device { id, generation }
            | Self::DeviceV2 { id, generation }
            | Self::DeviceV3 { id, generation } => match version {
                3 => Self::DeviceV3 { id, generation },
                2 => Self::DeviceV2 { id, generation },
                _ => Self::Device { id, generation },
            },
            Self::Enrollment | Self::EnrollmentV2 | Self::EnrollmentV3 => match version {
                3 => Self::EnrollmentV3,
                2 => Self::EnrollmentV2,
                _ => Self::Enrollment,
            },
            p => p,
        }
    }

    pub fn actor(&self) -> Result<Uuid> {
        if let Self::Actor(id) = self {
            Ok(*id)
        } else {
            Err(Error::Forbidden)
        }
    }
}
#[derive(Clone, Debug)]
pub struct Scope {
    pub school: Option<Uuid>,
    pub id: Option<Uuid>,
}
#[derive(Clone, Debug)]
pub enum Query {
    Configurations,
    Configuration,
    ConfigurationReleases,
    ConfigurationDownload,
    RunProgress,
    RunRevisionPreview,
    Targets,
    Strategies,
    Strategy,
    StrategyRuns,
    Devices,
    Device,
    Accounts,
    AccountPolicy,
    Groups,
    GroupPolicy,
    Capabilities,
    Settings,
    Tasks,
    Task,
    Items,
    Events,
    Session,
    Claim,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_is_cross_channel_stable() {
        assert_eq!(normalize(" \r\ne\u{301}\r\nx \n").unwrap(), "é\nx");
        assert_eq!(digest("é").unwrap(), digest("e\u{301}").unwrap());
        assert!(normalize("\0").is_err());
        assert!(normalize(&"中".repeat(1334)).is_err());
    }
    #[test]
    fn hard_policy_limits_include_pause() {
        for (v, a, ok) in [
            (0, true, true),
            (30, true, true),
            (50, true, true),
            (51, true, false),
            (30, false, true),
            (31, false, false),
            (-1, false, false),
        ] {
            assert_eq!(policy(v, a).is_ok(), ok);
        }
    }
    #[test]
    fn executable_fields_and_unknown_inputs_are_rejected() {
        assert!(
            serde_json::from_value::<Command>(
                serde_json::json!({"operation":"claim","binding_revision":1,"script":"evil"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<ContentSource>(
                serde_json::json!({"kind":"text","text_content":"x","shell":"x"})
            )
            .is_err()
        );
    }
}

/// Wire evidence is a send permission, never a platform-delivered/read status.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendPermit {
    pub id: Uuid,
    pub claim_id: Uuid,
    pub invocation_id: Uuid,
    pub fence: i64,
    pub snapshot_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permit_token: Option<String>,
    pub not_before: chrono::DateTime<chrono::Utc>,
    pub execute_before: chrono::DateTime<chrono::Utc>,
    pub server_time: chrono::DateTime<chrono::Utc>,
}
