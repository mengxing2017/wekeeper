//! Configured generic UI flow. No version/installation snapshot is required.
use super::{FlowV2, Pacing, Selector, Transition, validate_text};
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPath {
    pub kind: String,
    pub opener: Selector,
    pub search_input: Option<Selector>,
    pub application_row: Selector,
    pub max_scrolls: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowV3 {
    pub schema_version: u32,
    pub protocol_version: u8,
    pub engine_version: String,
    pub app_key: String,
    pub capability: String,
    pub application_label: String,
    pub package_name: String,
    pub selection_policy: String,
    pub result_order: String,
    pub title_rule: String,
    pub results_stable_ms: i64,
    pub page_timeout_ms: i64,
    pub pages: BTreeMap<String, Selector>,
    pub controls: BTreeMap<String, Selector>,
    pub navigation: Vec<Transition>,
    pub launcher_paths: Vec<DesktopPath>,
    pub pacing: Pacing,
    pub max_attempts: u32,
    pub max_total_steps: u32,
    pub max_total_scrolls: u32,
    pub max_total_duration_ms: i64,
    pub max_targets: u32,
}
impl FlowV3 {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 3
                && self.protocol_version == 9
                && self.engine_version == "otter-ui/3",
            "通用流程须使用 schema 3、协议 9 和 otter-ui/3",
        )?;
        require(
            self.package_name == "com.tencent.wework",
            "当前执行能力仅支持企业微信群文字",
        )?;
        require(
            matches!(self.title_rule.as_str(), "exact" | "trailing_member_count"),
            "请选择标题核对规则",
        )?;
        // Reuse the semantic validator, not the v8 wire identity or launcher model.
        let core = FlowV2 {
            schema_version: 3,
            protocol_version: 9,
            engine_version: self.engine_version.clone(),
            app_key: self.app_key.clone(),
            capability: self.capability.clone(),
            application_label: self.application_label.clone(),
            package_name: self.package_name.clone(),
            selection_policy: self.selection_policy.clone(),
            result_order: self.result_order.clone(),
            results_stable_ms: self.results_stable_ms,
            page_timeout_ms: self.page_timeout_ms,
            pages: self.pages.clone(),
            controls: self.controls.clone(),
            navigation: self.navigation.clone(),
            launchers: vec![],
            pacing: self.pacing.clone(),
            max_attempts: self.max_attempts,
            max_total_steps: self.max_total_steps,
            max_total_scrolls: self.max_total_scrolls,
            max_total_duration_ms: self.max_total_duration_ms,
            max_targets: self.max_targets,
        };
        core.validate_generic()?;
        require(
            self.controls.contains_key("group_result_title"),
            "通用流程需要群结果标题控件",
        )?;
        require(
            self.launcher_paths.len() == 2,
            "请配置桌面搜索和全部应用两条完整路径",
        )?;
        let mut kinds = BTreeSet::new();
        for path in &self.launcher_paths {
            require(
                kinds.insert(path.kind.as_str())
                    && matches!(path.kind.as_str(), "search" | "all_apps")
                    && path.max_scrolls <= 64,
                "桌面路径重复或预算无效",
            )?;
            path.opener.validate()?;
            path.application_row.validate()?;
            require(
                path.opener.predicate.clickable == Some(true)
                    && path.application_row.predicate.clickable == Some(true),
                "桌面入口和应用项须可点击",
            )?;
            if path.kind == "search" {
                let input = path
                    .search_input
                    .as_ref()
                    .ok_or(crate::business::Error::Invalid(
                        "桌面搜索路径缺少输入框".into(),
                    ))?;
                input.validate()?;
                require(input.predicate.editable == Some(true), "桌面搜索框须可编辑")?;
            } else {
                require(path.search_input.is_none(), "全部应用路径不接受搜索框")?;
            }
        }
        validate_text(&self.pacing.baseline_evidence, 1, 2000, 8000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v9_wire_is_separate_and_cross_language_digest_is_frozen() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/shared-mobile-send-v9.json"
        ))
        .unwrap();
        let flow: FlowV3 = serde_json::from_value(fixture["flow"].clone()).unwrap();
        flow.validate().unwrap();
        assert_eq!(
            crate::distribution::domain::mobile_execution::shared_send_v2::digest(&fixture["flow"])
                .unwrap(),
            fixture["flow_digest"].as_str().unwrap()
        );
        assert!(serde_json::from_value::<FlowV2>(fixture["flow"].clone()).is_err());
        let mut missing = flow.clone();
        missing.launcher_paths.pop();
        assert!(missing.validate().is_err());
        let mut unsafe_flow = flow;
        unsafe_flow.navigation.retain(|s| s.from != "chat");
        assert!(unsafe_flow.validate().is_err());
    }
}
