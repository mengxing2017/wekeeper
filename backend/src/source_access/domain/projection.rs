//! Version decisions and receipt wire types; no transport or database dependencies.
use super::*;
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Apply,
    Stale,
    Duplicate,
    Conflict,
}
pub fn decide(sequence: i64, fingerprint: &str, previous: Option<(i64, &str)>) -> Decision {
    match previous {
        None => Decision::Apply,
        Some((watermark, _)) if sequence < watermark => Decision::Stale,
        Some((watermark, old)) if sequence == watermark => {
            if fingerprint == old {
                Decision::Duplicate
            } else {
                Decision::Conflict
            }
        }
        _ => Decision::Apply,
    }
}
pub fn fingerprint(event: &Event) -> String {
    #[derive(Serialize)]
    struct Payload<'a> {
        algorithm: &'static str,
        event_type: &'a str,
        version: &'a str,
        title: &'a Option<String>,
        content: &'a Option<String>,
        detail_url: &'a str,
    }
    digest(
        &serde_json::to_vec(&Payload {
            algorithm: "source-payload/1",
            event_type: &event.event_type,
            version: &event.version,
            title: &event.title,
            content: &event.content,
            detail_url: &event.detail_url,
        })
        .expect("serializing fixed payload"),
    )
}
/// Initial exact-text policy evaluator. Rule management and normalized/local matching belong to volume 06.
pub fn blocked(event: &Event, terms: &[String]) -> bool {
    terms.iter().filter(|v| !v.is_empty()).any(|term| {
        event.title.as_deref().unwrap_or("").contains(term)
            || event.content.as_deref().unwrap_or("").contains(term)
    })
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
    IgnoredStale,
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
    pub retryable: bool,
    pub sending: Option<Sending>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_sequence: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecheckInput {
    pub revision: i64,
    pub reason: String,
}
pub enum Command {
    List {
        route: String,
        page: i64,
    },
    Detail {
        route: String,
        receipt: Uuid,
    },
    Recheck {
        route: String,
        receipt: Uuid,
        input: RecheckInput,
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
        event.content = Some(format!("{} ", event.content.unwrap()));
        assert_ne!(fingerprint(&event), expected);
        assert!(blocked(&event, &["中文".into()]));
        assert!(!blocked(&event, &[]));
    }
    #[test]
    fn versions_never_use_arrival_time() {
        assert_eq!(decide(7, "a", Some((8, "b"))), Decision::Stale);
        assert_eq!(decide(8, "b", Some((8, "b"))), Decision::Duplicate);
        assert_eq!(decide(8, "a", Some((8, "b"))), Decision::Conflict);
        assert_eq!(decide(10, "a", Some((8, "b"))), Decision::Apply);
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
        let typed: Receipt = serde_json::from_value(value["data"]["attributes"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(typed).unwrap(),
            value["data"]["attributes"]
        );
    }
}
