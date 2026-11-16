//! Platform-owned sender and preview contracts.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OfficialSender {
    pub id: Uuid,

    pub userid: String,
    pub display_name: String,
    pub member_verified: bool,
    pub member_active: bool,
    pub account_active: bool,
    pub binding_revision: i64,
    pub qualification_revision: i64,
    pub enabled: bool,
    pub eligible: bool,
    pub reason: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SenderQualificationInput {
    pub enabled: bool,
    pub revision: i64,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Preview {
    pub id: Uuid,
    pub revision: i64,
    pub digest: String,
    pub expires_at: String,
    pub content: String,
    pub target_count: i64,
    pub channel: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sender_and_preview_wire_shape() {
        let p = Preview {
            id: Uuid::nil(),
            revision: 1,
            digest: "hash".into(),
            expires_at: "time".into(),
            content: "text".into(),
            target_count: 2,
            channel: "wecom".into(),
        };
        let v = serde_json::to_value(p).unwrap();
        assert_eq!(v["channel"], "wecom");
        assert_eq!(v["target_count"], 2);
        assert!(
            serde_json::from_value::<SenderQualificationInput>(
                serde_json::json!({"enabled":false,"revision":1})
            )
            .is_ok()
        );
    }
}
