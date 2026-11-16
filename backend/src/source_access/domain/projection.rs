//! Text fingerprints, internal processing views and machine acceptance facts.
use super::*;
pub fn fingerprint(event: &Event) -> String {
    #[derive(Serialize)]
    struct Payload<'a> {
        algorithm: &'static str,
        event_type: &'a str,
        content: &'a str,
    }
    digest(
        &serde_json::to_vec(&Payload {
            algorithm: "source-payload/2",
            event_type: &event.event_type,
            content: &event.content,
        })
        .expect("serializing fixed payload"),
    )
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sending {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub unknown: i64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Received,
    Applied,
    Rejected,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub event_id: Uuid,
    pub status: ReceiptStatus,
    pub reason_code: Option<String>,
    pub processing_stage: String,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub sending: Option<Sending>,
}
/// Machine-visible immutable acceptance fact, independent of processing outcomes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceReceipt {
    pub event_id: Uuid,
    pub status: AcceptanceStatus,
    pub received_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Serialize, Deserialize)]
pub enum AcceptanceStatus {
    #[serde(rename = "received")]
    Received,
}
pub enum Command {
    List {
        route: String,
        page: crate::business::Page,
    },
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_fingerprint_is_independent_of_transport_identity_and_preserves_text() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-projection.json"
        ))
        .unwrap();
        let mut event: Event = serde_json::from_value(fixture["event"].clone()).unwrap();
        let expected = fingerprint(&event);
        assert_eq!(expected, fixture["sha256"]);
        event.event_id = Uuid::new_v4();
        event.occurred_at = chrono::Utc::now();
        assert_eq!(fingerprint(&event), expected);
        event.content.push(' ');
        assert_ne!(fingerprint(&event), expected);
    }
    #[test]
    fn all_receipt_projection_states_roundtrip_with_typed_rust_serialization() {
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-receipt-projection.json"
        ))
        .unwrap();
        for value in fixtures {
            let typed: Receipt = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), value);
        }
    }
    #[test]
    fn receipt_shared_fixture() {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-receipt-v2.json"
        ))
        .unwrap();
        let typed: AcceptanceReceipt =
            serde_json::from_value(value["data"]["attributes"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(typed).unwrap(),
            value["data"]["attributes"]
        );
    }
}
