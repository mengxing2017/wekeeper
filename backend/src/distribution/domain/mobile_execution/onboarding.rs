//! Product release references, authenticated environment reports and administrator progress.
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub android_api: i64,
    pub engine_version: String,
    pub application_status: String,
    pub package_name: String,
    pub version_name: Option<String>,
    pub version_code: Option<i64>,
    pub instance_ref: Option<String>,
    pub launcher_package: Option<String>,
    pub launcher_version: Option<i64>,
}
impl Environment {
    pub fn validate(&self) -> Result<()> {
        require(
            (26..=100).contains(&self.android_api)
                && self.engine_version == "otter-ui/2"
                && self.package_name == "com.tencent.wework"
                && matches!(
                    self.application_status.as_str(),
                    "confirmed" | "unconfirmed" | "unsupported_instances"
                ),
            "设备环境字段无效",
        )?;
        for text in [
            &self.version_name,
            &self.instance_ref,
            &self.launcher_package,
        ]
        .into_iter()
        .flatten()
        {
            require(
                !text.is_empty() && text.len() <= 256 && !text.chars().any(char::is_control),
                "设备环境字段无效",
            )?;
        }
        require(
            self.version_code.is_none_or(|v| v > 0) && self.launcher_version.is_none_or(|v| v > 0),
            "设备应用版本无效",
        )?;
        require(
            self.application_status != "confirmed"
                || (self.version_code.is_some()
                    && self.version_name.is_some()
                    && self.instance_ref.is_some()
                    && self.launcher_package.is_some()
                    && self.launcher_version.is_some()),
            "尚未确认实际应用环境",
        )
    }
    pub fn matches(&self, supported: &Value) -> bool {
        // Instance identity belongs to this phone and is bound by its snapshot, not release metadata.
        let mut actual = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(object) = actual.as_object_mut() {
            object.remove("instance_ref");
        }
        self.application_status == "confirmed" && actual == *supported
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningInput {
    pub device_id: Uuid,
    pub profile_release_id: Uuid,
    pub environment_snapshot_id: Uuid,
    pub expected_revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportInput {
    pub device_id: Uuid,
    pub note: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportUpdateInput {
    pub expected_revision: i64,
    pub state: String,
    pub response: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelInput {
    pub expected_revision: i64,
}
fn blocker(code: &str, message: &str, owner: &str, action: &str) -> Value {
    json!({"code":code,"message":message,"owner":owner,"next_action":action})
}
pub fn progress(mut row: Value) -> Value {
    let mut blockers = vec![];
    if row["online"] != true {
        blockers.push(blocker(
            "device_offline",
            "手机尚未连接平台",
            "手机现场操作人",
            "在手机启用长期托管，恢复连接后继续本次开通",
        ));
    }
    if row["release_count"] == 0 {
        blockers.push(blocker(
            "product_not_provided",
            "当前产品版本尚未提供此业务能力，暂不能开通",
            "产品研发",
            "提交适配支持请求，查看处理进度",
        ));
    }
    if row["environment_fresh"] != true {
        blockers.push(blocker(
            "environment_required",
            "实际应用环境待核对",
            "手机现场操作人",
            "在手机本机业务能力页面刷新环境与开通进度",
        ));
    } else if row["matching_releases"]
        .as_array()
        .is_none_or(|v| v.len() != 1)
    {
        blockers.push(blocker(
            "environment_unsupported",
            "此手机的实际应用环境暂未适配或匹配不唯一",
            "产品研发",
            "提交适配支持请求",
        ));
    }
    let opening = &row["opening"];
    if opening.is_null()
        && row["matching_releases"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
        && row["environment_fresh"] == true
    {
        blockers.push(blocker(
            "opening_required",
            "尚未发送本机确认申请",
            "平台资源管理员",
            "核对环境后发送到手机确认",
        ));
    } else if !opening.is_null() {
        let valid = row["environment"]["id"] == opening["environment_snapshot_id"]
            && row["release_available"] == true;
        if !valid {
            blockers.push(blocker(
                "opening_changed",
                "环境已变化或适配已撤回，需重新开通",
                "平台资源管理员",
                "取消原申请，刷新环境后重新开通",
            ));
        }
        if row["consent_current"] != true {
            blockers.push(blocker(
                "phone_consent_required",
                "等待手机原生确认",
                "手机现场操作人",
                "在本机业务能力页面查看范围并确认",
            ));
        }
        if row["field_check_current_passed"] != true {
            blockers.push(blocker(
                "field_check_required",
                "现场无发送检查尚未通过或依据已变化",
                "学校管理员与手机现场人员",
                "学校邀请已绑定目标，平台发起检查后由手机原生确认开始",
            ));
        }
    }
    blockers.push(blocker(
        "execution_not_delivered",
        "平台暂未开放手机任务执行",
        "平台产品方",
        "等待共享调度、执行租约与单次许可完成验收",
    ));
    row["blockers"] = json!(blockers);
    row["execution_ready"] = json!(false);
    row["delivery_state"] = json!("onboarding_only");
    row["local_device_state"] = json!("unconfirmed");
    row["can_open"] = json!(
        row["device_enabled"] == true
            && row["environment_fresh"] == true
            && row["matching_releases"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
            && row["opening"].is_null()
    );
    row
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_raw_flow_and_self_asserted_approval() {
        let good = json!({"device_id":Uuid::from_u128(1),"profile_release_id":Uuid::from_u128(2),"environment_snapshot_id":Uuid::from_u128(3),"expected_revision":0});
        assert!(serde_json::from_value::<OpeningInput>(good.clone()).is_ok());
        for key in [
            "flow",
            "accepted",
            "enabled",
            "execution_ready",
            "school_id",
        ] {
            let mut bad = good.clone();
            bad[key] = json!(true);
            assert!(serde_json::from_value::<OpeningInput>(bad).is_err());
        }
    }
    #[test]
    fn current_field_check_does_not_promote_execution_and_old_check_remains_blocked() {
        let row = json!({"release_count":1,"online":true,"device_enabled":true,"environment_fresh":true,"opening":{"id":"opening","environment_snapshot_id":"env"},"environment":{"id":"env"},"matching_releases":[{}],"release_available":true,"consent_current":true,"field_check_current_passed":true});
        let ready = progress(row.clone());
        assert_eq!(ready["execution_ready"], false);
        assert!(
            !ready["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "field_check_required")
        );
        let mut stale = row;
        stale["field_check_current_passed"] = json!(false);
        assert!(
            progress(stale)["blockers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["code"] == "field_check_required")
        );
    }
    #[test]
    fn no_product_has_an_owner_and_cannot_open() {
        let p = progress(
            json!({"release_count":0,"online":true,"device_enabled":true,"environment_fresh":false,"opening":null,"matching_releases":[]}),
        );
        assert_eq!(p["can_open"], false);
        assert_eq!(p["execution_ready"], false);
        assert_eq!(p["blockers"][0]["owner"], "产品研发");
    }
    #[test]
    fn native_environment_fixture_round_trips_without_browser_claims() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/shared-mobile-environment-report.json"
        ))
        .unwrap();
        let attributes = fixture["data"]["attributes"].clone();
        let parsed: Environment = serde_json::from_value(attributes.clone()).unwrap();
        parsed.validate().unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), attributes);
        for field in [
            "device_id",
            "credential_generation",
            "accepted",
            "verified",
            "school_id",
        ] {
            let mut forged = attributes.clone();
            forged[field] = json!(true);
            assert!(serde_json::from_value::<Environment>(forged).is_err());
        }
    }
    #[test]
    fn exact_environment_only() {
        let e = Environment {
            android_api: 36,
            engine_version: "otter-ui/2".into(),
            application_status: "confirmed".into(),
            package_name: "com.tencent.wework".into(),
            version_name: Some("1".into()),
            version_code: Some(1),
            instance_ref: Some("instance".into()),
            launcher_package: Some("home".into()),
            launcher_version: Some(1),
        };
        e.validate().unwrap();
        let mut matrix = serde_json::to_value(&e).unwrap();
        matrix.as_object_mut().unwrap().remove("instance_ref");
        assert!(e.matches(&matrix));
        matrix["version_code"] = json!(2);
        assert!(!e.matches(&matrix));
    }
}
