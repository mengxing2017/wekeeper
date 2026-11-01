//! Platform-owned material contracts. No forum entities, database or HTTP types.
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceReference {
    pub namespace: String,
    pub external_organization_id: String,
    pub external_content_id: String,
    pub version: String,
}
impl SourceReference {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.namespace,
            &self.external_organization_id,
            &self.external_content_id,
            &self.version,
        ] {
            require(
                !value.trim().is_empty() && value.len() <= 200,
                "来源引用无效",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Material {
    pub reference: SourceReference,
    pub title: String,
    pub content: String,
    pub detail_url: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OfficialSender {
    pub id: Uuid,
    pub school_id: Uuid,
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
    fn references_are_namespaced_and_wire_is_not_a_forum_post() {
        let a: SourceReference = serde_json::from_value(serde_json::json!({"namespace":"test.notice","external_organization_id":"1","external_content_id":"1","version":"v2"})).unwrap();
        a.validate().unwrap();
        let mut b = a.clone();
        b.namespace = "test.other".into();
        assert_ne!(a, b);
        assert!(serde_json::from_value::<SourceReference>(serde_json::json!({"namespace":"x","external_organization_id":"1","external_content_id":"1","version":"1","confirmed":true})).is_err());
        let value = serde_json::to_value(a).unwrap();
        assert_eq!(value["external_content_id"], "1");
    }
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
                serde_json::json!({"enabled":false,"revision":1,"school_id":"other"})
            )
            .is_err()
        );
    }
}
