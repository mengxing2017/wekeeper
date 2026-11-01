//! Immutable phone task preview rules. A confirmation is not a send permission.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPreviewInput {
    pub device_id: Uuid,
    pub target_ids: Vec<Uuid>,
    pub message: String,
}
impl TaskPreviewInput {
    pub fn validate(&self, flow_max_targets: u32) -> Result<()> {
        require(!self.device_id.is_nil(), "请选择设备")?;
        require(
            !self.target_ids.is_empty()
                && self.target_ids.len() <= 100
                && self.target_ids.len() <= flow_max_targets as usize,
            "目标数量超过流程预算或未选择目标",
        )?;
        require(
            self.target_ids.iter().all(|id| !id.is_nil())
                && self.target_ids.iter().collect::<BTreeSet<_>>().len() == self.target_ids.len(),
            "同一投递意图内目标不得重复",
        )?;
        // Preserve the exact body, including intentional newlines and surrounding spaces.
        require(
            !self.message.trim().is_empty()
                && self.message.len() <= 4000
                && !self
                    .message
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
            "消息必须为非空纯文本且不超过 4000 UTF-8 字节",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfirmationInput {
    pub preview_id: Uuid,
    pub preview_digest: String,
    pub confirmed: bool,
}
impl TaskConfirmationInput {
    pub fn validate(&self) -> Result<()> {
        require(
            self.confirmed
                && !self.preview_id.is_nil()
                && self.preview_digest.len() == 64
                && self.preview_digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "请核对完整预览并明确确认",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_order_body_and_budget_are_preserved_without_identity_inference() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let mut v = TaskPreviewInput {
            device_id: Uuid::now_v7(),
            target_ids: vec![b, a],
            message: " 原文\n正文\t ".into(),
        };
        v.validate(2).unwrap();
        assert_eq!(v.target_ids, vec![b, a]);
        assert_eq!(v.message, " 原文\n正文\t ");
        assert!(v.validate(1).is_err());
        v.target_ids = vec![a, a];
        assert!(v.validate(2).is_err());
        v.target_ids = vec![a];
        v.message = "中".repeat(1333) + "a";
        v.validate(1).unwrap();
        v.message.push('a');
        assert!(v.validate(1).is_err());
        for body in [" \n\t", "正文\0", "正文\u{1b}"] {
            v.message = body.into();
            assert!(v.validate(1).is_err());
        }
    }
    #[test]
    fn confirmation_requires_explicit_decision_and_strict_new_fields() {
        let mut v = TaskConfirmationInput {
            preview_id: Uuid::now_v7(),
            preview_digest: "a".repeat(64),
            confirmed: false,
        };
        assert!(v.validate().is_err());
        v.confirmed = true;
        v.validate().unwrap();
        let mut wire = serde_json::to_value(v).unwrap();
        wire["sender_account_id"] = serde_json::json!(Uuid::now_v7());
        assert!(serde_json::from_value::<TaskConfirmationInput>(wire).is_err());
    }
}
