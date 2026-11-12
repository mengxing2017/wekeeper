//! Channel-neutral data for a distribution delegation.
//!
//! Receiving a delegation does not itself authorize execution: the receiving
//! channel must verify the authorization against its own workspace and policy.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Immutable facts supplied to a channel when a message is delegated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRequest {
    pub delegation_id: Uuid,
    pub intake_id: Uuid,
    pub intake_revision: i64,
    pub correlation_id: Uuid,
    pub content: String,
    pub content_digest: String,
    pub authorization_ref: Uuid,
}

impl DispatchRequest {
    /// Validate structural facts and the SHA-256 digest of the exact UTF-8
    /// content. The accepting use case must independently verify authorization.
    pub fn is_well_formed(&self) -> bool {
        !self.delegation_id.is_nil()
            && !self.intake_id.is_nil()
            && !self.correlation_id.is_nil()
            && !self.authorization_ref.is_nil()
            && self.intake_revision > 0
            && !self.content.is_empty()
            && self.content_digest == format!("{:x}", Sha256::digest(self.content.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_requires_stable_identifiers_and_content() {
        let mut request = DispatchRequest {
            delegation_id: Uuid::new_v4(),
            intake_id: Uuid::new_v4(),
            intake_revision: 1,
            correlation_id: Uuid::new_v4(),
            content: "message".into(),
            content_digest: format!("{:x}", Sha256::digest(b"message")),
            authorization_ref: Uuid::new_v4(),
        };
        assert!(request.is_well_formed());
        request.content.push('!');
        assert!(!request.is_well_formed());
        request.content.pop();
        request.delegation_id = Uuid::nil();
        assert!(!request.is_well_formed());
        request.delegation_id = Uuid::new_v4();
        request.intake_revision = 0;
        assert!(!request.is_well_formed());
    }

    #[test]
    fn delegation_rejects_routing_data() {
        let mut value = serde_json::to_value(DispatchRequest {
            delegation_id: Uuid::new_v4(),
            intake_id: Uuid::new_v4(),
            intake_revision: 1,
            correlation_id: Uuid::new_v4(),
            content: "message".into(),
            content_digest: format!("{:x}", Sha256::digest(b"message")),
            authorization_ref: Uuid::new_v4(),
        })
        .unwrap();
        value["route_id"] = serde_json::json!("route");
        assert!(serde_json::from_value::<DispatchRequest>(value).is_err());
    }
}
