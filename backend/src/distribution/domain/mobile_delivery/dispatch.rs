//! v5 plans contain reusable UI semantics, never a pre-uploaded installation identity.
use super::*;
use std::collections::BTreeSet;

pub const DISPATCH_PROTOCOL: u32 = 5;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowClient {
    pub package_name: String,
}
impl RuleClient for FlowClient {
    const SCHEMA: u32 = 2;
    const ENGINE: &'static str = "otter-ui/2";
    fn validate_rule_client(&self) -> Result<()> {
        if self.package_name == "*" {
            Ok(())
        } else {
            validate_package(&self.package_name)
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowSpecialization {
    pub key: String,
    pub minimum_version_code: i64,
    pub maximum_version_code: i64,
    pub rules: UiRules<FlowClient>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchFlowV1 {
    pub schema_version: u32,
    pub app_key: String,
    pub capability: String,
    pub application_label: String,
    pub configuration: ConfigurationInput<FlowClient>,
    pub launchers: Vec<LauncherFlow<FlowClient>>,
    pub specializations: Vec<FlowSpecialization>,
    pub max_attempts: usize,
    pub max_total_steps: usize,
    pub max_total_scrolls: usize,
    pub max_total_duration_ms: i64,
    pub recoverable_errors: Vec<String>,
}
impl DispatchFlowV1 {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1 && self.app_key == "wecom" && self.capability == "group_text",
            "流程能力尚不支持",
        )?;
        label(&self.application_label)?;
        require(
            serde_json::to_vec(self).map_err(|_| Error::Conflict)?.len() <= 1_048_576,
            "流程超过引擎容量",
        )?;
        self.configuration.validate()?;
        validate_package(&self.configuration.rules.client.package_name)?;
        require(
            !self.launchers.is_empty()
                && self.launchers.len() <= 16
                && self.specializations.len() <= 16
                && (1..=32).contains(&self.max_attempts)
                && (1..=4096).contains(&self.max_total_steps)
                && (1..=512).contains(&self.max_total_scrolls)
                && (1..=86_400_000).contains(&self.max_total_duration_ms),
            "通用流程或预算缺失",
        )?;
        let mut packages = BTreeSet::new();
        for launcher in &self.launchers {
            launcher.validate()?;
            require(
                packages.insert(&launcher.client.package_name),
                "桌面候选重复",
            )?;
            for step in launcher.search_steps.iter().chain(&launcher.all_apps_steps) {
                require(
                    step.delay_ms >= self.configuration.baseline.navigation_min_ms,
                    "桌面节奏低于人工基准",
                )?;
            }
        }
        let mut keys = BTreeSet::new();
        for (index, candidate) in self.specializations.iter().enumerate() {
            label(&candidate.key)?;
            require(
                candidate.key != "generic"
                    && keys.insert(&candidate.key)
                    && candidate.minimum_version_code > 0
                    && candidate.maximum_version_code >= candidate.minimum_version_code,
                "专项范围无效",
            )?;
            require(
                self.specializations[..index].iter().all(|v| {
                    candidate.maximum_version_code < v.minimum_version_code
                        || candidate.minimum_version_code > v.maximum_version_code
                }),
                "专项版本范围重叠",
            )?;
            let mut config = self.configuration.clone();
            config.rules = candidate.rules.clone();
            config.validate()?;
            require(
                candidate.rules.client.package_name == self.configuration.rules.client.package_name,
                "专项不能切换应用",
            )?;
        }
        let unique: BTreeSet<_> = self.recoverable_errors.iter().collect();
        require(
            unique.len() == self.recoverable_errors.len()
                && self.recoverable_errors.iter().all(|v| {
                    matches!(
                        v.as_str(),
                        "navigation_failed"
                            | "control_unavailable"
                            | "navigation_path_unavailable"
                            | "application_not_found"
                            | "unexpected_page"
                            | "page_changed"
                            | "page_timeout"
                    )
                }),
            "降级错误类型不允许",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchTarget {
    pub assignment_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchInput {
    pub name: String,
    pub text_content: String,
    pub interval_seconds: i64,
    pub targets: Vec<DispatchTarget>,
}
impl DispatchInput {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        normalize(&self.text_content)?;
        require(
            (1..=31_536_000).contains(&self.interval_seconds)
                && !self.targets.is_empty()
                && self.targets.len() <= 100,
            "批次范围或间隔无效",
        )?;
        let mut assignments = BTreeSet::new();
        let mut groups = BTreeSet::new();
        for target in &self.targets {
            require(
                assignments.insert(target.assignment_id)
                    && !target.group_ids.is_empty()
                    && target.group_ids.len() <= 4096,
                "执行者重复或群范围无效",
            )?;
            for group in &target.group_ids {
                require(groups.insert(*group), "一个批次中的群只能有一个执行归属")?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureVersions {
    pub application_name: Option<String>,
    pub application_code: Option<i64>,
    pub engine: Option<String>,
    pub android: Option<String>,
    pub launcher: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchBudgetUsage {
    pub attempts: u64,
    pub steps: u64,
    pub scrolls: u64,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchFailure {
    #[serde(default)]
    pub attempted_paths: Vec<String>,
    #[serde(default)]
    pub budget: Option<DispatchBudgetUsage>,
    pub event_id: Uuid,
    pub item_id: Uuid,
    pub fence: i64,
    pub plan_digest: String,
    pub path_key: String,
    pub step: String,
    pub reason: String,
    pub send_state: String,
    pub versions: Option<FailureVersions>,
}
impl DispatchFailure {
    pub fn validate(&self) -> Result<()> {
        require(self.attempted_paths.len() <= 32, "尝试路径过多")?;
        for path in &self.attempted_paths {
            text(path, 1, 100)?;
        }
        if let Some(b) = &self.budget {
            require(
                b.attempts <= 33
                    && b.steps <= 4097
                    && b.scrolls <= 513
                    && b.elapsed_ms <= 172_800_000,
                "预算记录无效",
            )?;
        }
        for value in [&self.path_key, &self.step, &self.reason] {
            text(value, 1, 100)?;
        }
        require(
            self.plan_digest.len() == 64
                && self
                    .plan_digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "计划摘要无效",
        )?;
        require(
            self.fence > 0 && matches!(self.send_state.as_str(), "not_sent" | "unknown"),
            "失败发送状态无效",
        )?;
        if let Some(v) = &self.versions {
            for s in [&v.application_name, &v.engine, &v.android, &v.launcher]
                .into_iter()
                .flatten()
            {
                text(s, 1, 200)?;
            }
            require(v.application_code.is_none_or(|n| n > 0), "版本号无效")?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessApplicationScope {
    pub format: String,
    pub origin: String,
    pub device_id: Uuid,
    pub sender_account_id: Uuid,
    pub schools: Vec<Uuid>,
    pub app_key: String,
    pub capability: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessAuthorizationReceipt {
    pub authorization_id: Uuid,
    pub revision: i64,
    pub state: String,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub scope: BusinessApplicationScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_revision: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceSetupSchool {
    pub id: Uuid,
    pub name: String,
    pub dispatch_enabled: bool,
    pub assignment_ready: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceSetupStatus {
    pub id: Uuid,
    pub protocol_version: u32,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub device_name: String,
    pub account_name: Option<String>,
    pub sender_account_id: Option<Uuid>,
    pub binding_revision: Option<i64>,
    pub scope_source: String,
    pub schools: Vec<DeviceSetupSchool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchManualApplication {
    pub label: String,
    pub client: FlowClient,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DispatchCommand {
    RequestValidation {
        device_id: Uuid,
        flow_id: Option<Uuid>,
        manual_application: Option<DispatchManualApplication>,
        kind: ValidationKind,
        path_key: String,
        launcher_index: Option<usize>,
    },
    AcceptValidation {
        expected_revision: i64,
        generation: Uuid,
        authorization_id: Uuid,
    },
    ValidationEvent {
        event: ValidationEvent,
    },
    CancelValidation {
        expected_revision: i64,
    },
    CreateFlow {
        flow: Box<DispatchFlowV1>,
        pacing_profile_id: Uuid,
    },
    CreatePacing {
        name: String,
        school_id: Uuid,
        validation_ids: Vec<Uuid>,
    },
    PublishFlow {
        school_id: Uuid,
        validation_id: Uuid,
    },
    RevokeFlow {
        expected_revision: i64,
        reason: String,
    },
    SetEnabled {
        enabled: bool,
        reviewed_history: bool,
    },
    SaveGroup {
        name: String,
        device_ids: Vec<Uuid>,
    },
    Assign {
        device_ids: Vec<Uuid>,
        group_id: Option<Uuid>,
        release_id: Uuid,
    },
    Preview {
        input: DispatchInput,
    },
    Confirm {
        preview_digest: String,
        reviewed: bool,
    },
    ControlBatch {
        expected_revision: i64,
        command: String,
    },
    Receipt {
        plan_digest: String,
    },
    PreparationLease {
        plan_digest: String,
        authorization_id: Uuid,
    },
    Stage {
        generation: Uuid,
        item_id: Uuid,
        stage: String,
        path_key: String,
    },
    Failure {
        failure: DispatchFailure,
    },
    Diagnostic {
        failure_id: Uuid,
        event_id: Uuid,
        versions: FailureVersions,
    },
    Authorization {
        receipt: BusinessAuthorizationReceipt,
    },
    Heartbeat {
        binding_revision: i64,
        sender_account_id: Uuid,
        permissions_ready: bool,
    },
    Prepare {
        fence: i64,
        snapshot_digest: String,
        observed_account_id: Uuid,
        observed_group_id: Uuid,
        plan_digest: String,
        path_digest: String,
        checks_passed: bool,
        verification: VisibleVerification,
    },
    RebuildRemaining {
        expected_revision: i64,
        release_id: Uuid,
        reviewed: bool,
    },
}
#[derive(Clone, Debug)]
pub enum DispatchQuery {
    Flows,
    Releases,
    Pacing,
    DeviceGroups,
    Assignments,
    Batches,
    Batch,
    Inbox,
    AuthorizationContext,
    SetupStatus,
    Failures,
    Validations,
    DeviceValidations,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unbound_device_setup_serialization_is_explicit_and_read_only() {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/device-setup-status-v5.json"
        ))
        .unwrap();
        let status: DeviceSetupStatus = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(status.protocol_version, 5);
        assert!(status.sender_account_id.is_none() && status.schools.is_empty());
        assert_eq!(serde_json::to_value(status).unwrap(), value);
    }
    #[test]
    fn business_receipt_binding_revision_remains_backward_compatible() {
        let value = serde_json::json!({"authorization_id":Uuid::nil(),"revision":1,"state":"effective","granted_at":"2026-10-31T00:00:00Z","revoked_at":null,"scope":{"format":"business_application_scope/2","origin":"https://keeper.test","device_id":Uuid::nil(),"sender_account_id":Uuid::nil(),"schools":[Uuid::nil()],"app_key":"wecom","capability":"group_text"}});
        let old: BusinessAuthorizationReceipt = serde_json::from_value(value.clone()).unwrap();
        assert!(old.binding_revision.is_none());
        assert_eq!(serde_json::to_value(old).unwrap(), value);
        let mut current = value;
        current["binding_revision"] = serde_json::json!(7);
        let receipt: BusinessAuthorizationReceipt =
            serde_json::from_value(current.clone()).unwrap();
        assert_eq!(receipt.binding_revision, Some(7));
        assert_eq!(serde_json::to_value(receipt).unwrap(), current);
    }
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/automation-dispatch-flow-v5.json"
        ))
        .unwrap()
    }
    #[test]
    fn generic_flow_needs_no_installed_versions_and_rejects_unsafe_fallback() {
        let value = fixture();
        let f: DispatchFlowV1 = serde_json::from_value(value.clone()).unwrap();
        f.validate().unwrap();
        assert_eq!(serde_json::to_value(f).unwrap(), value);
        let mut bad = value.clone();
        bad["configuration"]["rules"]["client"]["version_code"] = serde_json::json!(9);
        assert!(serde_json::from_value::<DispatchFlowV1>(bad).is_err());
        let mut bad = value.clone();
        bad["recoverable_errors"] = serde_json::json!(["existing_draft"]);
        assert!(
            serde_json::from_value::<DispatchFlowV1>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
        let mut bad = value;
        bad["launchers"] = serde_json::json!([]);
        assert!(
            serde_json::from_value::<DispatchFlowV1>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    #[test]
    fn specializations_cannot_overlap_or_remove_generic_flow() {
        let mut f = fixture();
        let rules = f["configuration"]["rules"].clone();
        f["specializations"] = serde_json::json!([{"key":"a","minimum_version_code":1,"maximum_version_code":10,"rules":rules},{"key":"b","minimum_version_code":10,"maximum_version_code":20,"rules":rules}]);
        assert!(
            serde_json::from_value::<DispatchFlowV1>(f)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    #[test]
    fn v5_canonical_plan_and_cross_device_target_ownership() {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/automation-canonical-v5.json"
        ))
        .unwrap();
        assert_eq!(v["canonical"], v["value"].to_string());
        assert_eq!(v["sha256"], automation_digest(&v["value"]));
        let p: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/automation-dispatch-plan-v5.json"
        ))
        .unwrap();
        assert_eq!(p["content_digest"], automation_digest(&p["body"]));
        let g = Uuid::new_v4();
        let mut input = DispatchInput {
            name: "test".into(),
            text_content: "测试".into(),
            interval_seconds: 60,
            targets: vec![
                DispatchTarget {
                    assignment_id: Uuid::new_v4(),
                    group_ids: vec![g],
                },
                DispatchTarget {
                    assignment_id: Uuid::new_v4(),
                    group_ids: vec![g],
                },
            ],
        };
        assert!(input.validate().is_err());
        input.targets.pop();
        input.validate().unwrap();
    }
}
