//! Typed automation protocol. Package/version identify an OS environment, never a chat or UI node.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub const AUTOMATION_PROTOCOL: u32 = 4;
pub const AUTOMATION_ENGINE: &str = "otter-automation/1";
pub const AUTOMATION_CAPABILITIES: &[&str] = &[
    "launcher_search/1",
    "launcher_all_apps/1",
    "application_binding/1",
    "validation_jobs/1",
    "ordered_groups/1",
];
pub const AUTHORIZATION_EXTENSION: &str = "standing_authorization/1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedApplication {
    pub binding_id: Uuid,
    pub app_key: String,
    pub instance_ref: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedSchool {
    pub school_id: Uuid,
    pub applications: Vec<AuthorizedApplication>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionScope {
    pub origin: String,
    pub device_id: Uuid,
    pub sender_account_id: Uuid,
    pub schools: Vec<AuthorizedSchool>,
    pub purpose: String,
    pub data_policy: String,
    pub object_scope: String,
}
impl ExecutionScope {
    pub fn validate(&self) -> Result<()> {
        require(
            self.purpose == "company_business/1"
                && self.data_policy == "minimal_visible_results/1"
                && self.object_scope == "approved_school_groups/1",
            "授权用途或数据范围不支持",
        )?;
        require(
            !self.schools.is_empty() && self.schools.len() <= 100,
            "授权学校数量无效",
        )?;
        let mut seen = BTreeSet::new();
        for school in &self.schools {
            require(
                seen.insert(school.school_id) && school.applications.len() <= 16,
                "学校重复或应用过多",
            )?;
            let mut apps = BTreeSet::new();
            for app in &school.applications {
                require(
                    apps.insert(app.binding_id) && app.app_key == "wecom",
                    "授权应用无效",
                )?;
                text(&app.instance_ref, 1, 200)?;
            }
        }
        Ok(())
    }
    pub fn covers(&self, school: Uuid, binding: Option<&AuthorizedApplication>) -> bool {
        self.schools
            .iter()
            .any(|s| s.school_id == school && binding.is_none_or(|b| s.applications.contains(b)))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAuthorizationReceipt {
    pub authorization_id: Uuid,
    pub revision: i64,
    pub state: String,
    pub notice_version: String,
    pub notice_digest: String,
    pub identity_assurance: String,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub maintenance: bool,
    pub scope: ExecutionScope,
}
impl ExecutionAuthorizationReceipt {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            (1..=9_007_199_254_740_991).contains(&self.revision)
                && self.notice_version == "internal_business/1"
                && self.identity_assurance == "local_action",
            "授权凭证版本或身份等级无效",
        )?;
        require(
            self.notice_digest.len() == 64
                && self
                    .notice_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "告知摘要无效",
        )?;
        require(
            matches!(self.state.as_str(), "effective" | "revoked")
                && (self.state == "revoked") == self.revoked_at.is_some(),
            "撤销状态无效",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledApplication {
    pub app_key: String,
    pub instance_ref: String,
    pub label: String,
    pub client: UiClient,
    pub instance_visibility: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentReport {
    pub generation: i64,
    pub launcher: UiClient,
    pub applications: Vec<InstalledApplication>,
    pub engine_version: String,
    pub capabilities: Vec<String>,
    pub notifications_ready: bool,
    pub accessibility_ready: bool,
    pub unlocked: bool,
    pub maintenance_mode: bool,
}
impl EnvironmentReport {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=9_007_199_254_740_991).contains(&self.generation) && self.applications.len() <= 16,
            "环境代次或候选数量无效",
        )?;
        validate_client(&self.launcher)?;
        text(&self.engine_version, 1, 100)?;
        require(
            self.capabilities.len() <= 32
                && self.capabilities.iter().collect::<BTreeSet<_>>().len()
                    == self.capabilities.len(),
            "能力数量过多或重复",
        )?;
        for capability in &self.capabilities {
            text(capability, 1, 100)?;
        }
        let mut refs = BTreeSet::new();
        for app in &self.applications {
            require(
                ["wecom", "wechat", "dingtalk", "douyin"].contains(&app.app_key.as_str()),
                "仅接收目录应用候选",
            )?;
            require(refs.insert(&app.instance_ref), "安装实例重复")?;
            text(&app.instance_ref, 1, 200)?;
            label(&app.label)?;
            validate_client(&app.client)?;
            require(
                ["unique", "ambiguous", "unavailable"].contains(&app.instance_visibility.as_str()),
                "实例可见性无效",
            )?;
        }
        Ok(())
    }
    pub fn supports_execution(&self) -> bool {
        self.engine_version == AUTOMATION_ENGINE
            && self.notifications_ready
            && self.accessibility_ready
            && self.unlocked
            && AUTOMATION_CAPABILITIES
                .iter()
                .all(|c| self.capabilities.iter().any(|v| v == c))
    }
}
fn validate_client(c: &UiClient) -> Result<()> {
    text(&c.package_name, 1, 200)?;
    text(&c.version_name, 1, 100)?;
    require(
        (1..=9_007_199_254_740_991).contains(&c.version_code),
        "安装版本无效",
    )
}
/// Semantic navigation only. No coordinates, resource IDs, arbitrary text or executable code.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum LauncherAction {
    Home,
    Click { role: String },
    SearchApplication { role: String },
    ScrollForward { role: String },
    VerifyApplication,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherStep {
    pub from: String,
    pub to: String,
    pub operation: LauncherAction,
    pub delay_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherFlow<C = UiClient> {
    pub client: C,
    pub pages: BTreeMap<String, UiPage>,
    pub roles: BTreeMap<String, UiSelector>,
    pub search_steps: Vec<LauncherStep>,
    pub all_apps_steps: Vec<LauncherStep>,
    pub max_steps: usize,
    pub timeout_ms: i64,
    pub max_scrolls: usize,
    pub local_application_results_only: bool,
}
impl<C: RuleClient> LauncherFlow<C> {
    pub fn validate(&self) -> Result<()> {
        self.client.validate_rule_client()?;
        require(
            !self.pages.is_empty() && self.pages.len() <= 32 && self.roles.len() <= 64,
            "桌面页面或角色数量无效",
        )?;
        require(
            (1..=128).contains(&self.max_steps)
                && (1..=60_000).contains(&self.timeout_ms)
                && (1..=128).contains(&self.max_scrolls),
            "桌面预算无效",
        )?;
        require(self.local_application_results_only, "仅允许本地应用结果")?;
        require(
            !self.search_steps.is_empty() || !self.all_apps_steps.is_empty(),
            "至少配置一种桌面路径",
        )?;
        for (name, page) in &self.pages {
            label(name)?;
            page.marker.validate()?;
            require(
                page.required.len() <= 16 && page.forbidden.len() <= 16,
                "页面条件过多",
            )?;
            for q in page.required.iter().chain(&page.forbidden) {
                q.validate()?;
            }
        }
        for (name, role) in &self.roles {
            label(name)?;
            role.validate()?;
        }
        require(
            self.search_steps.is_empty()
                || self
                    .search_steps
                    .iter()
                    .any(|s| matches!(s.operation, LauncherAction::SearchApplication { .. })),
            "搜索路径必须包含本地应用搜索",
        )?;
        for steps in [&self.search_steps, &self.all_apps_steps] {
            if steps.is_empty() {
                continue;
            }
            require(
                steps.len() <= self.max_steps
                    && matches!(
                        steps.first().map(|s| &s.operation),
                        Some(LauncherAction::Home)
                    )
                    && matches!(
                        steps.last().map(|s| &s.operation),
                        Some(LauncherAction::VerifyApplication)
                    ),
                "桌面路径须从主屏幕开始并以应用核验结束",
            )?;
            require(
                steps
                    .iter()
                    .filter(|s| matches!(s.operation, LauncherAction::Home))
                    .count()
                    == 1
                    && steps
                        .iter()
                        .filter(|s| matches!(s.operation, LauncherAction::VerifyApplication))
                        .count()
                        == 1,
                "主屏幕和应用核验只能出现在路径边界",
            )?;
            for (i, step) in steps.iter().enumerate() {
                require(
                    self.pages.contains_key(&step.from)
                        && self.pages.contains_key(&step.to)
                        && (0..self.timeout_ms).contains(&step.delay_ms),
                    "桌面步骤页面或等待无效",
                )?;
                if i > 0 {
                    require(steps[i - 1].to == step.from, "桌面步骤不连续")?;
                }
                match &step.operation {
                    LauncherAction::Click { role }
                    | LauncherAction::SearchApplication { role }
                    | LauncherAction::ScrollForward { role } => {
                        let selector = self
                            .roles
                            .get(role)
                            .ok_or(Error::Invalid("桌面角色缺失".into()))?;
                        require(
                            !["send", "input", "message"].contains(&role.as_str()),
                            "桌面不能引用发送角色",
                        )?;
                        match step.operation {
                            LauncherAction::SearchApplication { .. } => require(
                                selector.predicate.editable == Some(true),
                                "应用搜索角色须可编辑",
                            )?,
                            LauncherAction::Click { .. } => require(
                                selector.predicate.clickable == Some(true),
                                "点击角色须可点击",
                            )?,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentBody {
    Launcher { flow: LauncherFlow },
    Workflow { rules: UiRules },
    Policy { policy: ExecutionPolicy },
    Baseline { validation_ids: Vec<Uuid> },
}
impl ComponentBody {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Launcher { .. } => "launcher",
            Self::Workflow { .. } => "workflow",
            Self::Policy { .. } => "policy",
            Self::Baseline { .. } => "baseline",
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Launcher { flow } => flow.validate(),
            Self::Workflow { rules } => rules.validate(),
            Self::Policy { policy: p } => {
                require(
                    p.minimum_manual_samples > 0
                        && p.minimum_manual_samples <= 100
                        && p.max_targets > 0
                        && p.max_targets <= 4096
                        && p.account_limit >= 0
                        && p.group_limit >= 0
                        && (1..=1024).contains(&p.max_navigation_steps),
                    "政策容量无效",
                )?;
                for v in [
                    p.account_cooldown_ms,
                    p.content_cooldown_ms,
                    p.default_group_interval_ms,
                    p.minimum_group_interval_ms,
                    p.input_codepoint_delay_ms,
                    p.input_settle_ms,
                ] {
                    require((0..=31_536_000_000).contains(&v), "政策时间超出容量")?;
                }
                require(
                    (1..=86_400_000).contains(&p.session_max_duration_ms)
                        && (1..=31_536_000_000).contains(&p.quota_window_ms)
                        && (1..=60_000).contains(&p.page_timeout_ms)
                        && (1..=60_000).contains(&p.observation_timeout_ms)
                        && p.poll_interval_ms > 0
                        && p.poll_interval_ms <= p.page_timeout_ms.min(p.observation_timeout_ms),
                    "政策期限无效",
                )
            }
            Self::Baseline { validation_ids } => require(
                !validation_ids.is_empty()
                    && validation_ids.len() <= 100
                    && validation_ids.iter().collect::<BTreeSet<_>>().len() == validation_ids.len(),
                "须选择不同的真实人工计时记录",
            ),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessInput {
    pub app_binding_id: Uuid,
    pub capability_id: Uuid,
    pub name: String,
    pub group_ids: Vec<Uuid>,
    pub text_content: String,
    pub interval_seconds: i64,
    pub schedule: String,
}
impl BusinessInput {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        normalize(&self.text_content)?;
        require(
            !self.group_ids.is_empty()
                && self.group_ids.len() <= 4096
                && self.group_ids.iter().collect::<BTreeSet<_>>().len() == self.group_ids.len(),
            "目标群数量无效或重复",
        )?;
        require(
            (1..=31_536_000).contains(&self.interval_seconds) && self.schedule == "manual_once",
            "等待秒数无效；仅支持手动运行一次",
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationKind {
    InspectEnvironment,
    OpenApplication,
    InspectRoles,
    MeasureManual,
}
impl ValidationKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::InspectEnvironment => "inspect_environment",
            Self::OpenApplication => "open_application",
            Self::InspectRoles => "inspect_roles",
            Self::MeasureManual => "measure_manual",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualTiming {
    pub group_interval_ms: i64,
    pub navigation_ms: i64,
    pub input_codepoint_ms: i64,
    pub input_settle_ms: i64,
    pub observation_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEvent {
    pub event_id: Uuid,
    pub generation: Uuid,
    pub expected_revision: i64,
    pub stage: String,
    pub outcome: String,
    pub role_matches: BTreeMap<String, u32>,
    pub timing: Option<ManualTiming>,
    pub path: Option<String>,
}
impl ValidationEvent {
    pub fn validate(&self, kind: ValidationKind) -> Result<()> {
        require(
            [
                "checking_environment",
                "returning_to_launcher",
                "matching_launcher",
                "searching_application",
                "browsing_all_applications",
                "opening_application",
                "verifying_application",
                "inspecting_roles",
                "measuring_manual",
                "ready",
                "interrupted",
            ]
            .contains(&self.stage.as_str()),
            "未知验证阶段",
        )?;
        require(
            ["running", "succeeded", "failed", "interrupted"].contains(&self.outcome.as_str()),
            "验证结果无效",
        )?;
        require(
            self.role_matches.len() <= 192 && self.role_matches.values().all(|n| *n <= 4096),
            "角色匹配结果过多",
        )?;
        for role in self.role_matches.keys() {
            label(role)?;
        }
        if self.outcome == "succeeded" {
            require(self.stage == "ready", "成功必须已完成核验")?;
            if kind == ValidationKind::OpenApplication {
                require(
                    matches!(self.path.as_deref(), Some("search" | "all_apps")),
                    "测试打开必须记录实际桌面路径",
                )?;
            }
            if kind == ValidationKind::InspectRoles {
                require(
                    !self.role_matches.is_empty() && self.role_matches.values().all(|n| *n == 1),
                    "角色必须唯一匹配",
                )?;
            }
            if kind == ValidationKind::MeasureManual {
                require(self.timing.is_some(), "缺少人工计时")?;
            }
        }
        if let Some(t) = &self.timing {
            require(kind == ValidationKind::MeasureManual, "仅人工计时接受样本")?;
            for v in [
                t.group_interval_ms,
                t.navigation_ms,
                t.input_codepoint_ms,
                t.input_settle_ms,
                t.observation_ms,
            ] {
                require((1..=31_536_000_000).contains(&v), "人工计时无效")?;
            }
        }
        if let Some(path) = &self.path {
            require(
                ["search", "all_apps"].contains(&path.as_str()),
                "桌面路径无效",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutomationCommand {
    RecordAuthorization {
        receipt: Box<ExecutionAuthorizationReceipt>,
    },
    AcceptStandingValidation {
        expected_revision: i64,
        generation: Uuid,
        authorization_id: Uuid,
    },
    RejectValidation {
        expected_revision: i64,
        generation: Uuid,
    },
    ReportEnvironment {
        report: EnvironmentReport,
    },
    BindApplication {
        device_id: Uuid,
        app_id: Uuid,
        environment_id: Uuid,
        instance_ref: String,
        expected_revision: i64,
    },
    RequestValidation {
        device_id: Uuid,
        app_binding_id: Option<Uuid>,
        kind: ValidationKind,
        component_ids: Vec<Uuid>,
    },
    CancelValidation {
        expected_revision: i64,
    },
    AcceptValidation {
        expected_revision: i64,
        generation: Uuid,
    },
    ReportValidation {
        event: ValidationEvent,
    },
    CreateComponent {
        name: String,
        body: Box<ComponentBody>,
    },
    PublishComponent {
        school_id: Uuid,
        device_id: Option<Uuid>,
        validation_id: Uuid,
        expected_revision: i64,
    },
    RevokeRelease {
        expected_revision: i64,
        reason: String,
    },
    PreviewBusiness {
        input: BusinessInput,
    },
    ConfirmBusiness {
        expected_revision: i64,
        preview_digest: String,
        reviewed: bool,
    },
    PrepareExecution {},
    ReportPreparation {
        generation: Uuid,
        stage: String,
    },
    ApplicationReady {
        generation: Uuid,
        manifest_digest: String,
        environment_id: Uuid,
    },
}
#[derive(Clone, Debug)]
pub enum AutomationQuery {
    AuthorizationContext,
    Apps,
    Applications,
    Bindings,
    Validations,
    Validation,
    Components,
    Releases,
    Runs,
    Run,
    ValidationJobs,
    Resolution,
}
/// serde_json's sorted map encoding is the v4 UTF-8 wire canonicalization; no floating values enter manifests.
pub fn automation_digest(value: &serde_json::Value) -> String {
    hash(&value.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn standing_authorization_is_typed_scoped_and_distinct_from_local_acceptance() {
        let id = Uuid::new_v4();
        let scope = ExecutionScope {
            origin: "https://example.test".into(),
            device_id: id,
            sender_account_id: id,
            schools: vec![AuthorizedSchool {
                school_id: id,
                applications: vec![AuthorizedApplication {
                    binding_id: id,
                    app_key: "wecom".into(),
                    instance_ref: "local".into(),
                }],
            }],
            purpose: "company_business/1".into(),
            data_policy: "minimal_visible_results/1".into(),
            object_scope: "approved_school_groups/1".into(),
        };
        scope.validate().unwrap();
        assert!(scope.covers(id, Some(&scope.schools[0].applications[0])));
        assert!(!scope.covers(Uuid::new_v4(), None));
        let r = ExecutionAuthorizationReceipt {
            authorization_id: id,
            revision: 1,
            state: "effective".into(),
            notice_version: "internal_business/1".into(),
            notice_digest: "a".repeat(64),
            identity_assurance: "local_action".into(),
            granted_at: chrono::Utc::now(),
            revoked_at: None,
            maintenance: false,
            scope,
        };
        r.validate().unwrap();
        let mut v = json!(r);
        v["identity_assurance"] = json!("corporate_verified");
        assert!(
            serde_json::from_value::<ExecutionAuthorizationReceipt>(v)
                .unwrap()
                .validate()
                .is_err()
        );
        let mut c = json!({"action":"accept_standing_validation","expected_revision":1,"generation":id,"authorization_id":id});
        assert!(serde_json::from_value::<AutomationCommand>(c.clone()).is_ok());
        c["source"] = json!("user_clicked");
        assert!(serde_json::from_value::<AutomationCommand>(c).is_err());
        assert!(
            serde_json::from_value::<AutomationCommand>(
                json!({"action":"reject_validation","expected_revision":1,"generation":id})
            )
            .is_ok()
        );
    }
    #[test]
    fn typed_business_rejects_json_scripts_fractional_times_and_duplicate_groups() {
        let mut v = serde_json::json!({"app_binding_id":Uuid::nil(),"capability_id":Uuid::nil(),"name":"业务","group_ids":[Uuid::nil()],"text_content":"测试","interval_seconds":60,"schedule":"manual_once"});
        assert!(
            serde_json::from_value::<BusinessInput>(v.clone())
                .unwrap()
                .validate()
                .is_ok()
        );
        v["interval_seconds"] = serde_json::json!(1.5);
        assert!(serde_json::from_value::<BusinessInput>(v.clone()).is_err());
        v["interval_seconds"] = serde_json::json!(i64::MAX);
        assert!(
            serde_json::from_value::<BusinessInput>(v.clone())
                .unwrap()
                .validate()
                .is_err()
        );
        v["rules"] = serde_json::json!({"script":"click"});
        assert!(serde_json::from_value::<BusinessInput>(v).is_err());
    }
    #[test]
    fn timing_and_real_launcher_path_are_required() {
        let mut e = ValidationEvent {
            event_id: Uuid::nil(),
            generation: Uuid::nil(),
            expected_revision: 1,
            stage: "ready".into(),
            outcome: "succeeded".into(),
            role_matches: BTreeMap::new(),
            timing: None,
            path: None,
        };
        assert!(e.validate(ValidationKind::OpenApplication).is_err());
        e.path = Some("intent".into());
        assert!(e.validate(ValidationKind::OpenApplication).is_err());
        e.path = Some("search".into());
        assert!(e.validate(ValidationKind::OpenApplication).is_ok());
        assert!(e.validate(ValidationKind::MeasureManual).is_err());
    }
    #[test]
    fn canonical_bytes_are_sorted_utf8_without_whitespace() {
        let a: serde_json::Value = serde_json::from_str("{\"中\":\"测试\\n\",\"a\":1}").unwrap();
        assert_eq!(a.to_string(), "{\"a\":1,\"中\":\"测试\\n\"}");
        assert_eq!(automation_digest(&a), hash("{\"a\":1,\"中\":\"测试\\n\"}"));
    }
}
