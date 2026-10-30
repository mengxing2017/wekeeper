//! Declarative UI configuration. No scripts, resource IDs, coordinates or external identities.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const CONFIGURATION_SCHEMA: u32 = 1;
pub const ENGINE_VERSION: &str = "otter-ui/1";
pub const ENGINE_CAPABILITIES: &[&str] = &[
    "non_id_query/1",
    "bounded_navigation/1",
    "visible_verification/1",
    "ordered_groups/1",
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPredicate {
    pub text: Option<String>,
    pub description: Option<String>,
    pub class_name: Option<String>,
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub editable: Option<bool>,
    pub clickable: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiSelector {
    pub predicate: UiPredicate,
    pub parent: Option<UiPredicate>,
    pub ancestor: Option<UiPredicate>,
    pub ancestor_depth: u32,
    #[serde(default)]
    pub all_of: Vec<UiPredicate>,
    #[serde(default)]
    pub any_of: Vec<UiPredicate>,
}
impl UiSelector {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=32).contains(&self.ancestor_depth)
                && self.all_of.len() <= 16
                && self.any_of.len() <= 16,
            "节点条件超出引擎容量",
        )?;
        for p in std::iter::once(&self.predicate)
            .chain(self.parent.iter())
            .chain(self.ancestor.iter())
            .chain(&self.all_of)
            .chain(&self.any_of)
        {
            for s in [&p.text, &p.description, &p.class_name]
                .into_iter()
                .flatten()
            {
                text(s, 1, 500)?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPage {
    pub marker: UiSelector,
    pub required: Vec<UiSelector>,
    pub forbidden: Vec<UiSelector>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiTransition {
    pub from: String,
    pub to: String,
    pub control: String,
    pub min_delay_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiLandmarks {
    pub entry: String,
    pub account: String,
    pub search: String,
    pub results: String,
    pub chat: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiClient {
    pub package_name: String,
    pub version_name: String,
    pub version_code: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiRules {
    pub schema_version: u32,
    pub engine_version: String,
    pub required_capabilities: Vec<String>,
    pub client: UiClient,
    pub pages: BTreeMap<String, UiPage>,
    pub controls: BTreeMap<String, UiSelector>,
    pub navigation_graph: Vec<UiTransition>,
    pub landmarks: UiLandmarks,
    pub group_type_labels: BTreeMap<String, String>,
    /// A validated title position can display a member-count suffix. Real names are compared first.
    pub member_count_suffix: String,
}
pub const REQUIRED_CONTROLS: &[&str] = &[
    "company",
    "account",
    "account_detail",
    "search_input",
    "group_tab",
    "result_title",
    "results_complete",
    "chat_title",
    "group_type",
    "owner",
    "input",
    "send",
    "message_row",
    "message_text",
    "outgoing",
    "failed",
    "pending",
];
impl UiRules {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == CONFIGURATION_SCHEMA && self.engine_version == ENGINE_VERSION,
            "规则 schema 或引擎版本不支持",
        )?;
        let capabilities: BTreeSet<_> = self
            .required_capabilities
            .iter()
            .map(String::as_str)
            .collect();
        require(
            capabilities == ENGINE_CAPABILITIES.iter().copied().collect(),
            "引擎能力要求不完整或不支持",
        )?;
        require(
            self.client.version_code > 0
                && self.client.package_name.split('.').count() >= 2
                && self.client.package_name.split('.').all(|s| {
                    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                }),
            "客户端包或版本无效",
        )?;
        text(&self.client.version_name, 1, 100)?;
        require(
            !self.pages.is_empty()
                && self.pages.len() <= 64
                && self.controls.len() <= 128
                && self.navigation_graph.len() <= 256,
            "规则超出引擎容量",
        )?;
        for name in [
            &self.landmarks.entry,
            &self.landmarks.account,
            &self.landmarks.search,
            &self.landmarks.results,
            &self.landmarks.chat,
        ] {
            require(self.pages.contains_key(name), "流程页面缺失")?;
        }
        for p in self.pages.values() {
            p.marker.validate()?;
            require(
                p.required.len() <= 16 && p.forbidden.len() <= 16,
                "页面条件过多",
            )?;
            for q in p.required.iter().chain(&p.forbidden) {
                q.validate()?;
            }
        }
        for q in self.controls.values() {
            q.validate()?;
        }
        for role in REQUIRED_CONTROLS {
            require(self.controls.contains_key(*role), "必需控件角色缺失")?;
        }
        let send = &self.controls["send"].predicate;
        require(
            send.visible == Some(true)
                && send.enabled == Some(true)
                && send.clickable == Some(true),
            "发送控件必须可见可用且可点击",
        )?;
        for role in ["input", "search_input"] {
            require(
                self.controls[role].predicate.editable == Some(true),
                "输入控件必须可编辑",
            )?;
        }
        let send = serde_json::to_value(&self.controls["send"]).map_err(|_| Error::Conflict)?;
        for edge in &self.navigation_graph {
            require(
                self.pages.contains_key(&edge.from)
                    && self.pages.contains_key(&edge.to)
                    && self.controls.contains_key(&edge.control)
                    && edge.control != "send"
                    && edge.min_delay_ms >= 0,
                "导航边或动作无效",
            )?;
            require(
                serde_json::to_value(&self.controls[&edge.control]).map_err(|_| Error::Conflict)?
                    != send,
                "导航不能指向发送控件",
            )?;
        }
        // Every page must have a bounded route back to the entry, and each workflow landmark must be reachable.
        for start in self.pages.keys() {
            require(
                self.reachable(start, &self.landmarks.entry),
                "页面不能返回起始页",
            )?;
        }
        for target in [
            &self.landmarks.account,
            &self.landmarks.search,
            &self.landmarks.results,
            &self.landmarks.chat,
        ] {
            require(
                self.reachable(&self.landmarks.entry, target),
                "流程页面不可达",
            )?;
        }
        require(
            ["none", "ascii", "full_width"].contains(&self.member_count_suffix.as_str()),
            "不支持的群名人数解析方式",
        )?;
        require(
            self.group_type_labels.len() == 2
                && self.group_type_labels.contains_key("internal")
                && self.group_type_labels.contains_key("external"),
            "需要内外部群可见文字映射",
        )?;
        for s in self.group_type_labels.values() {
            text(s, 1, 100)?;
        }
        Ok(())
    }
    fn reachable(&self, start: &str, target: &str) -> bool {
        let mut visited = BTreeSet::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            if node == target {
                return true;
            }
            if visited.insert(node) {
                pending.extend(
                    self.navigation_graph
                        .iter()
                        .filter(|e| e.from == node)
                        .map(|e| e.to.as_str()),
                );
            }
        }
        false
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    pub account_cooldown_ms: i64,
    pub content_cooldown_ms: i64,
    pub default_group_interval_ms: i64,
    pub minimum_group_interval_ms: i64,
    pub session_max_duration_ms: i64,
    pub quota_window_ms: i64,
    pub account_limit: i32,
    pub group_limit: i32,
    pub max_targets: usize,
    pub input_codepoint_delay_ms: i64,
    pub input_settle_ms: i64,
    pub page_timeout_ms: i64,
    pub observation_timeout_ms: i64,
    pub poll_interval_ms: i64,
    pub max_navigation_steps: usize,
    pub minimum_manual_samples: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualBaseline {
    pub version: String,
    pub samples: usize,
    pub human_group_min_interval_ms: i64,
    pub navigation_min_ms: i64,
    pub input_codepoint_min_ms: i64,
    pub input_settle_min_ms: i64,
    pub observation_min_ms: i64,
    pub evidence_ref: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationInput {
    pub name: String,
    pub rules: UiRules,
    pub policy: ExecutionPolicy,
    pub baseline: ManualBaseline,
    pub evidence_ref: String,
}
impl ConfigurationInput {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        text(&self.evidence_ref, 1, 500)?;
        self.rules.validate()?;
        let p = &self.policy;
        let b = &self.baseline;
        text(&b.version, 1, 100)?;
        text(&b.evidence_ref, 1, 500)?;
        // Bounds are resource/protocol capacity, not compiled business defaults.
        for v in [
            p.account_cooldown_ms,
            p.content_cooldown_ms,
            p.default_group_interval_ms,
            p.minimum_group_interval_ms,
            p.input_codepoint_delay_ms,
            p.input_settle_ms,
            b.human_group_min_interval_ms,
            b.navigation_min_ms,
            b.input_codepoint_min_ms,
            b.input_settle_min_ms,
            b.observation_min_ms,
        ] {
            require((0..=31_536_000_000).contains(&v), "时间参数超出引擎容量")?;
        }
        require(
            p.session_max_duration_ms > 0
                && p.session_max_duration_ms <= 86_400_000
                && p.quota_window_ms > 0
                && p.quota_window_ms <= 31_536_000_000
                && p.page_timeout_ms > 0
                && p.page_timeout_ms <= 60_000
                && p.observation_timeout_ms > 0
                && p.observation_timeout_ms <= 60_000
                && p.poll_interval_ms > 0
                && p.poll_interval_ms <= p.page_timeout_ms
                && p.poll_interval_ms <= p.observation_timeout_ms,
            "会话或观察参数无效",
        )?;
        require(
            (1..=4096).contains(&p.max_targets)
                && (1..=1024).contains(&p.max_navigation_steps)
                && p.account_limit >= 0
                && p.group_limit >= 0
                && p.minimum_manual_samples > 0
                && b.samples >= p.minimum_manual_samples,
            "额度、容量或人工样本不满足要求",
        )?;
        require(
            b.human_group_min_interval_ms > 0
                && p.default_group_interval_ms
                    >= p.minimum_group_interval_ms
                        .max(b.human_group_min_interval_ms)
                && p.input_codepoint_delay_ms >= b.input_codepoint_min_ms
                && p.input_settle_ms >= b.input_settle_min_ms
                && p.observation_timeout_ms >= b.observation_min_ms
                && b.observation_min_ms > 0,
            "操作节奏不得快于人工基准",
        )?;
        require(
            self.rules.navigation_graph.iter().all(|e| {
                e.min_delay_ms >= b.navigation_min_ms && e.min_delay_ms < p.page_timeout_ms
            }),
            "导航节奏或超时不满足人工基准",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/mobile-configuration-v3.json"
        ))
        .unwrap()
    }
    #[test]
    fn canonical_fixture_and_human_policy_are_shared_with_otter() {
        let v = fixture();
        let c: ConfigurationInput = serde_json::from_value(v["configuration"].clone()).unwrap();
        c.validate().unwrap();
        assert_eq!(
            super::super::hash(&serde_json::to_value(&c).unwrap().to_string()),
            v["content_digest"]
        );
        let mut faster = c.clone();
        faster.policy.input_codepoint_delay_ms = 1;
        assert!(faster.validate().is_err());
        let mut no_baseline = c.clone();
        no_baseline.baseline.samples = 0;
        assert!(no_baseline.validate().is_err());
        let mut unreachable = c.clone();
        unreachable
            .rules
            .navigation_graph
            .retain(|e| e.to != "home");
        assert!(unreachable.validate().is_err());
        let mut send_navigation = c.clone();
        send_navigation.rules.navigation_graph[0].control = "send".into();
        assert!(send_navigation.validate().is_err());
        let mut forged = fixture()["configuration"].clone();
        forged["rules"]["controls"]["input"]["predicate"]["resource_id"] =
            serde_json::json!("forbidden");
        assert!(serde_json::from_value::<ConfigurationInput>(forged).is_err());
    }
}
