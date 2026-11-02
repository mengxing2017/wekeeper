//! The sole new flow schema. Selectors describe visible UI, never account identity.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predicate {
    pub text: Option<String>,
    pub description: Option<String>,
    pub class_name: Option<String>,
    pub visible: bool,
    pub enabled: Option<bool>,
    pub editable: Option<bool>,
    pub clickable: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub predicate: Predicate,
    pub ancestor: Option<Predicate>,
    pub ancestor_depth: u32,
}
impl Selector {
    pub(super) fn validate(&self) -> Result<()> {
        require(
            (1..=32).contains(&self.ancestor_depth),
            "控件祖先范围必须为 1–32 层",
        )?;
        for p in std::iter::once(&self.predicate).chain(self.ancestor.iter()) {
            require(p.visible, "控件必须要求可见")?;
            require(
                p.text.is_some()
                    || p.description.is_some()
                    || p.class_name.is_some()
                    || p.editable == Some(true),
                "控件条件不能为空",
            )?;
            for text in [&p.text, &p.description, &p.class_name]
                .into_iter()
                .flatten()
            {
                validate_text(text, 1, 500, 2000)?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub control: String,
    pub delay_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    pub package_name: String,
    pub search_input: Selector,
    pub application_row: Selector,
    pub max_scrolls: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pacing {
    pub navigation_min_ms: i64,
    pub before_send_min_ms: i64,
    pub between_targets_min_ms: i64,
    pub baseline_evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowV2 {
    pub schema_version: u32,
    pub engine_version: String,
    pub protocol_version: u8,
    pub app_key: String,
    pub capability: String,
    pub application_label: String,
    pub package_name: String,
    pub selection_policy: String,
    pub result_order: String,
    pub results_stable_ms: i64,
    pub page_timeout_ms: i64,
    pub pages: BTreeMap<String, Selector>,
    pub controls: BTreeMap<String, Selector>,
    pub navigation: Vec<Transition>,
    pub launchers: Vec<Launcher>,
    pub pacing: Pacing,
    pub max_attempts: u32,
    pub max_total_steps: u32,
    pub max_total_scrolls: u32,
    pub max_total_duration_ms: i64,
    pub max_targets: u32,
}
fn package(value: &str) -> Result<()> {
    require(
        value.len() <= 256
            && value.split('.').count() >= 2
            && value.split('.').all(|s| {
                s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                    && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            }),
        "应用包名无效",
    )
}
pub const FLOW_CHECKS: &[&str] = &[
    "application_opened",
    "group_container_only",
    "first_group_visual_order",
    "results_stable",
    "chat_opened",
    "send_control_unique",
    "human_pacing",
    "no_send_performed",
];
impl FlowV2 {
    pub fn validate(&self) -> Result<()> {
        self.validate_protocol(6)
    }
    pub fn validate_shared(&self) -> Result<()> {
        self.validate_protocol(7)
    }
    pub fn validate_shared_send(&self) -> Result<()> {
        self.validate_protocol(8)?;
        self.validate_return_path()
    }
    fn validate_return_path(&self) -> Result<()> {
        let mut reached: BTreeSet<&str> = ["chat"].into();
        loop {
            let before = reached.len();
            for t in &self.navigation {
                if reached.contains(t.from.as_str())
                    && !matches!(t.to.as_str(), "chat" | "results")
                    && !matches!(
                        t.control.as_str(),
                        "input"
                            | "send"
                            | "group_result_row"
                            | "search_input"
                            | "message_row"
                            | "outgoing"
                            | "failed"
                            | "pending"
                    )
                {
                    reached.insert(&t.to);
                }
            }
            if reached.len() == before {
                break;
            }
        }
        require(
            reached.contains("search"),
            "v8 必须提供聊天页安全返回搜索页的真实导航路径",
        )
    }
    pub(super) fn validate_generic(&self) -> Result<()> {
        self.validate_protocol(9)?;
        self.validate_return_path()
    }
    fn validate_protocol(&self, protocol: u8) -> Result<()> {
        require(
            self.schema_version == if protocol == 9 { 3 } else { 2 }
                && self.protocol_version == protocol
                && self.engine_version
                    == if protocol == 9 {
                        "otter-ui/3"
                    } else {
                        "otter-ui/2"
                    }
                && self.app_key == "wecom"
                && self.capability == "group_text",
            "流程必须匹配指定业务协议和 schema v2 的企业微信群文字能力",
        )?;
        require(
            self.selection_policy == SELECTION_POLICY && self.result_order == "top_to_bottom",
            "必须仅选择群分类中按视觉顺序排列的首项",
        )?;
        validate_text(&self.application_label, 1, 128, 512)?;
        package(&self.package_name)?;
        require(
            serde_json::to_vec(self)
                .map_err(|_| crate::business::Error::Conflict)?
                .len()
                <= 1_048_576,
            "流程超过容量",
        )?;
        require(
            (100..=10_000).contains(&self.results_stable_ms)
                && (1000..=120_000).contains(&self.page_timeout_ms)
                && self.results_stable_ms < self.page_timeout_ms,
            "结果稳定等待或页面超时无效",
        )?;
        require(
            (1..=32).contains(&self.max_attempts)
                && (1..=4096).contains(&self.max_total_steps)
                && self.max_total_scrolls <= 512
                && (1000..=86_400_000).contains(&self.max_total_duration_ms)
                && (1..=100).contains(&self.max_targets),
            "执行预算无效",
        )?;
        require(
            (100..=60_000).contains(&self.pacing.navigation_min_ms)
                && (100..=60_000).contains(&self.pacing.before_send_min_ms)
                && (1000..=3_600_000).contains(&self.pacing.between_targets_min_ms),
            "执行节奏不能低于人工基准范围",
        )?;
        validate_text(&self.pacing.baseline_evidence, 1, 2000, 8000)?;
        require(
            self.pages.len() <= 64
                && self.controls.len() <= 128
                && self.navigation.len() <= 256
                && (protocol == 9 || !self.launchers.is_empty())
                && self.launchers.len() <= 16,
            "页面、控件或桌面流程数量无效",
        )?;
        for role in ["entry", "search", "results", "chat"] {
            require(
                self.pages.contains_key(role),
                "缺少入口、搜索、结果或群会话页面",
            )?;
        }
        for role in [
            "search_input",
            "group_tab",
            "group_results_container",
            "group_result_row",
            "results_complete",
            "chat_title",
            "input",
            "send",
            "message_row",
            "outgoing",
            "failed",
            "pending",
        ] {
            require(
                self.controls.contains_key(role),
                "缺少群分类、结果容器、结果项或发送观察控件",
            )?;
        }
        for (name, selector) in self.pages.iter().chain(self.controls.iter()) {
            validate_text(name, 1, 64, 256)?;
            selector.validate()?;
        }
        for role in ["search_input", "input"] {
            require(
                self.controls[role].predicate.editable == Some(true),
                "搜索及消息输入框必须可编辑",
            )?;
        }
        for role in ["send", "group_result_row"] {
            // Empty conversations may expose a disabled send control. v9 checks
            // its availability only after exact message preparation.
            if protocol == 9 && role == "send" {
                continue;
            }
            let p = &self.controls[role].predicate;
            require(
                p.enabled == Some(true) && p.clickable == Some(true),
                "发送按钮及群结果行必须可用且可点击",
            )?;
        }
        require(
            serde_json::to_value(&self.controls["group_results_container"]).ok()
                != serde_json::to_value(&self.controls["group_result_row"]).ok(),
            "群容器与群行不能使用相同控件条件",
        )?;
        // Sending is a separate single-use invocation, never a generic navigation step.
        for t in &self.navigation {
            require(
                self.pages.contains_key(&t.from)
                    && self.pages.contains_key(&t.to)
                    && self.controls.contains_key(&t.control)
                    && !matches!(t.control.as_str(), "send" | "group_result_row")
                    && (self.pacing.navigation_min_ms..=60_000).contains(&t.delay_ms),
                "导航引用无效、节奏过低或包含发送/首群点击",
            )?;
        }
        let mut reached: BTreeSet<&str> = ["entry"].into();
        loop {
            let before = reached.len();
            for t in &self.navigation {
                if reached.contains(t.from.as_str()) {
                    reached.insert(&t.to);
                }
            }
            if before == reached.len() {
                break;
            }
        }
        require(
            reached.contains("search") && reached.contains("results"),
            "入口必须能够导航至搜索和结果页",
        )?;
        let mut launchers = BTreeSet::new();
        for l in &self.launchers {
            package(&l.package_name)?;
            require(
                launchers.insert(&l.package_name) && l.max_scrolls <= 64,
                "桌面重复或滚动预算无效",
            )?;
            l.search_input.validate()?;
            l.application_row.validate()?;
            require(
                l.search_input.predicate.editable == Some(true)
                    && l.application_row.predicate.clickable == Some(true),
                "桌面搜索框或应用项不可操作",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowInput {
    pub name: String,
    pub flow: FlowV2,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationInput {
    pub flow_version_id: Uuid,
    pub device_id: Uuid,
    pub search_text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResultInput {
    pub challenge: String,
    pub flow_digest: String,
    pub protocol_version: u8,
    pub engine_version: String,
    pub package_name: String,
    pub version_code: i64,
    pub checks: BTreeMap<String, bool>,
    pub evidence: String,
}
impl ValidationResultInput {
    pub fn validate(&self) -> Result<()> {
        require(
            self.protocol_version == 6
                && self.engine_version == "otter-ui/2"
                && self.version_code > 0,
            "验证设备必须支持当前协议与引擎",
        )?;
        require(
            self.checks
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == FLOW_CHECKS.iter().copied().collect(),
            "设备验证检查项不完整",
        )?;
        // A send during validation must never produce a releasable result.
        validate_text(&self.evidence, 1, 2000, 8000)?;
        validate_text(&self.challenge, 1, 128, 512)?;
        require(self.flow_digest.len() == 64, "流程摘要无效")?;
        package(&self.package_name)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub flow_version_id: Uuid,
    pub validation_id: Uuid,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentInput {
    pub device_id: Uuid,
    pub release_id: Uuid,
    pub expected_revision: i64,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/mobile-execution-flow-v2.json"
        ))
        .unwrap()
    }
    #[test]
    fn v2_fixture_validates_and_rejects_legacy_identity_and_unbounded_selection() {
        let fixture = fixture();
        serde_json::from_value::<FlowV2>(fixture.clone())
            .unwrap()
            .validate()
            .unwrap();
        for (key, value) in [
            ("schema_version", serde_json::json!(1)),
            ("protocol_version", serde_json::json!(5)),
            ("selection_policy", serde_json::json!("unique")),
            ("result_order", serde_json::json!("tree_order")),
            ("max_targets", serde_json::json!(101)),
            ("results_stable_ms", serde_json::json!(0)),
        ] {
            let mut changed = fixture.clone();
            changed[key] = value;
            assert!(
                serde_json::from_value::<FlowV2>(changed)
                    .unwrap()
                    .validate()
                    .is_err(),
                "{key}"
            );
        }
        let mut identity = fixture.clone();
        identity["sender_account_id"] = serde_json::json!(Uuid::nil());
        assert!(serde_json::from_value::<FlowV2>(identity).is_err());
        let mut container = fixture.clone();
        container["controls"]
            .as_object_mut()
            .unwrap()
            .remove("group_results_container");
        assert!(
            serde_json::from_value::<FlowV2>(container)
                .unwrap()
                .validate()
                .is_err()
        );
        let mut send = fixture.clone();
        send["navigation"][0]["control"] = serde_json::json!("send");
        assert!(
            serde_json::from_value::<FlowV2>(send)
                .unwrap()
                .validate()
                .is_err()
        );
        let mut unreachable = fixture;
        unreachable["navigation"] = serde_json::json!([]);
        assert!(
            serde_json::from_value::<FlowV2>(unreachable)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    #[test]
    fn verification_requires_complete_check_set_and_records_failed_checks() {
        let mut v = ValidationResultInput {
            challenge: "nonce".into(),
            flow_digest: "a".repeat(64),
            protocol_version: 6,
            engine_version: "otter-ui/2".into(),
            package_name: "com.tencent.wework".into(),
            version_code: 1,
            checks: FLOW_CHECKS.iter().map(|k| (k.to_string(), true)).collect(),
            evidence: "检查页面，未发送".into(),
        };
        v.validate().unwrap();
        v.checks.insert("no_send_performed".into(), false);
        v.validate().unwrap(); // Failed evidence must be retained, never accepted as a passed validation.
        assert!(!v.checks.values().all(|v| *v));
        v.checks.remove("group_container_only");
        assert!(v.validate().is_err());
    }
}
