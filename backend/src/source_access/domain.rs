//! Published source-v2 wire and byte rules. No HTTP or database dependencies.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub const MEDIA_TYPE: &str = "application/vnd.api+json";
pub const MAX_BODY: usize = 1_048_576;
pub type Result<T> = std::result::Result<T, Fault>;
#[derive(Debug)]
pub struct Fault(pub &'static str);

/// Business identity frozen when a source message was accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedIdentity {
    pub platform_id: Uuid,
    pub unit_id: Uuid,
}
/// Backend-worker coordination identity; never sent to mobile devices.
#[derive(Clone, Debug)]
pub struct MessageHandoff {
    pub receipt_id: Uuid,
    pub platform_id: Uuid,
    pub claim_token: Uuid,
}
/// Committed source evidence; only intake can resolve this scoped record.
#[derive(Debug)]
pub struct AcceptedEvent {
    pub identity: AcceptedIdentity,
    pub receipt_id: Uuid,
    pub event: Event,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Document<T> {
    pub data: Resource<T>,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Resource<T> {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: Uuid,
    pub attributes: T,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: Uuid,
    pub schema_version: u32,
    pub route_id: String,
    pub event_type: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub content: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub route_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}
pub fn ensure(ok: bool, code: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(Fault(code)) }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn canonical(
    key: &str,
    method: &str,
    path: &str,
    timestamp: &str,
    nonce: &str,
    identity: &str,
    body: &[u8],
) -> String {
    format!(
        "2\n{key}\n{method}\n{path}\n{timestamp}\n{nonce}\n{identity}\n{}",
        digest(body)
    )
}
pub fn sign(secret: &[u8], text: &str) -> String {
    ring::hmac::sign(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret),
        text.as_bytes(),
    )
    .as_ref()
    .iter()
    .map(|v| format!("{v:02x}"))
    .collect()
}
pub fn verify(secret: &[u8], text: &str, hex: &str) -> Result<()> {
    ensure(
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "authentication_failed",
    )?;
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    ring::hmac::verify(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret),
        text.as_bytes(),
        &bytes,
    )
    .map_err(|_| Fault("authentication_failed"))
}
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
pub fn encode(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for b in bytes {
        acc = (acc << 8) | u32::from(*b);
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            result.push(ALPHABET[((acc >> bits) & 63) as usize] as char);
        }
    }
    if bits > 0 {
        result.push(ALPHABET[((acc << (6 - bits)) & 63) as usize] as char);
    }
    result
}
pub fn decode(value: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for b in value.bytes() {
        let n = ALPHABET
            .iter()
            .position(|v| *v == b)
            .ok_or(Fault("invalid_secret"))?;
        acc = (acc << 6) | n as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    ensure(encode(&out) == value, "invalid_secret")?;
    Ok(out)
}
pub fn secret(value: &str) -> Result<Vec<u8>> {
    let bytes = decode(value)?;
    ensure(bytes.len() >= 32, "invalid_secret")?;
    Ok(bytes)
}
pub fn window(timestamp: &str, now: i64) -> Result<i64> {
    let seconds = timestamp
        .parse::<i64>()
        .map_err(|_| Fault("invalid_request"))?;
    ensure(
        seconds.to_string() == timestamp && seconds >= 0,
        "invalid_request",
    )?;
    ensure(seconds.abs_diff(now) <= 300, "timestamp_expired")?;
    Ok(seconds)
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
impl Event {
    pub fn validate(&self) -> Result<()> {
        ensure(self.schema_version == 2, "unsupported_version")?;
        ensure(
            self.event_type == "content.upsert",
            "unsupported_event_type",
        )?;
        ensure(identifier(&self.route_id), "invalid_request")?;
        ensure(!self.content.trim().is_empty(), "content_empty")?;
        ensure(self.content.chars().count() <= 200, "content_too_long")?;
        Ok(())
    }
}
/// Inspect the event discriminator before validating the ordinary message body.
/// Withdrawals are rejected even when they do not contain a content field.
pub fn parse_event(body: &[u8]) -> Result<Document<Event>> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| Fault("invalid_request"))?;
    if let Some(kind) = value
        .pointer("/data/attributes/event_type")
        .and_then(serde_json::Value::as_str)
    {
        ensure(kind == "content.upsert", "unsupported_event_type")?;
    }
    serde_json::from_value(value).map_err(|_| Fault("forbidden_field"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_vectors() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/source-v2.json"
        ))
        .unwrap();
        for v in fixtures["vectors"].as_array().unwrap() {
            let body = v["body"].as_str().unwrap().as_bytes();
            let text = canonical(
                v["key_id"].as_str().unwrap(),
                v["method"].as_str().unwrap(),
                v["path"].as_str().unwrap(),
                v["timestamp"].as_str().unwrap(),
                v["nonce"].as_str().unwrap(),
                v["identity"].as_str().unwrap(),
                body,
            );
            assert_ne!(digest(&[body, b"!"].concat()), v["digest"]);
            assert_eq!(digest(body), v["digest"]);
            assert_eq!(text, v["canonical"]);
            let key = secret(v["secret"].as_str().unwrap()).unwrap();
            assert_eq!(sign(&key, &text), v["signature"]);
            verify(&key, &text, v["signature"].as_str().unwrap()).unwrap();
            assert!(verify(&key, &format!("{text}\n"), v["signature"].as_str().unwrap()).is_err());
        }
    }
    #[test]
    fn window_and_encoding() {
        for n in [-300, 300] {
            assert!(window(&(1000 + n).to_string(), 1000).is_ok());
        }
        for n in [-301, 301] {
            assert!(window(&(1000 + n).to_string(), 1000).is_err());
        }
        assert!(window("01000", 1000).is_err());
        assert!(secret("short").is_err());
        for n in 0..100 {
            let v = vec![255; n];
            assert_eq!(decode(&encode(&v)).unwrap(), v);
        }
    }
}
/// A missing end date removes only the expiry limit, never the status/start checks.
pub fn credential_available(
    status: &str,
    start: chrono::DateTime<chrono::Utc>,
    end: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    status == "active" && start <= now && end.is_none_or(|end| end > now)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminInput {
    pub key_id: Option<String>,
    pub unit_name: Option<String>,
    pub remark: Option<String>,
    pub status: Option<String>,
    pub revision: Option<i64>,
    pub business_unit_name: Option<String>,
    pub valid_from: Option<chrono::DateTime<chrono::Utc>>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub route_ids: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct UnitRouteResult {
    pub id: String,
    pub business_unit_id: Uuid,
}

#[cfg(test)]
mod unit_management_contract_tests {
    use super::{AdminInput, UnitRouteResult};

    #[test]
    fn unit_request_uses_shared_fixture_and_rejects_old_route_name() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/source-unit-management.json"
        ))
        .unwrap();
        let input: AdminInput = serde_json::from_value(fixture["request"].clone()).unwrap();
        assert_eq!(serde_json::to_value(input).unwrap(), fixture["serialized"]);
        assert!(
            serde_json::from_value::<AdminInput>(serde_json::json!({
                "key_id": "fixture-key", "route_name": "legacy"
            }))
            .is_err()
        );
        let result = UnitRouteResult {
            id: "fixture-route".into(),
            business_unit_id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
        };
        assert_eq!(serde_json::to_value(result).unwrap(), fixture["result"]);
    }
}
#[derive(Serialize)]
pub struct CredentialSecret {
    pub id: String,
    pub key_id: String,
    pub secret: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRevealInput {}
#[derive(Clone)]
pub struct Signed {
    pub key_id: String,
    pub timestamp: String,
    pub nonce: Uuid,
    pub identity: Uuid,
    pub signature: String,
    pub method: String,
    pub path: String,
    pub request_id: String,
}
#[cfg(test)]
mod boundary_tests {
    use super::*;
    use serde_json::json;
    fn message(content: &str) -> serde_json::Value {
        json!({"data":{"type":"source-events","id":Uuid::nil(),"attributes":{
            "event_id":Uuid::nil(),"schema_version":2,"route_id":"route-1",
            "event_type":"content.upsert","occurred_at":"2026-11-16T00:00:00Z","content":content
        }}})
    }
    #[test]
    fn complete_content_roundtrips_without_composition_or_url_validation() {
        for content in [
            "正文",
            " 标题\r\n正文\n#小程序://Linjian/示例 ",
            "http://localhost:5173/a?q=1#part",
            "https://example.test/a?q=1#part",
        ] {
            let value = message(content);
            let doc = parse_event(&serde_json::to_vec(&value).unwrap()).unwrap();
            doc.data.attributes.validate().unwrap();
            assert_eq!(doc.data.attributes.content, content);
            assert_eq!(serde_json::to_value(doc).unwrap(), value);
        }
        for removed in [
            "title",
            "detail_url",
            "external_content_id",
            "version",
            "sequence",
        ] {
            let mut value = message("正文");
            value["data"]["attributes"][removed] = json!("legacy");
            assert!(parse_event(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn content_bounds_use_unicode_scalars_and_reject_empty_content() {
        for content in ["", " \r\n\t"] {
            let doc = parse_event(&serde_json::to_vec(&message(content)).unwrap()).unwrap();
            assert_eq!(
                doc.data.attributes.validate().unwrap_err().0,
                "content_empty"
            );
        }
        for character in ["汉", "🌸", "a"] {
            let value = message(&character.repeat(200));
            let doc = parse_event(&serde_json::to_vec(&value).unwrap()).unwrap();
            doc.data.attributes.validate().unwrap();
            let value = message(&character.repeat(201));
            let doc = parse_event(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(
                doc.data.attributes.validate().unwrap_err().0,
                "content_too_long"
            );
        }
        for missing in [true, false] {
            let mut value = message("正文");
            if missing {
                value["data"]["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("content");
            } else {
                value["data"]["attributes"]["content"] = json!(null);
            }
            assert!(parse_event(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn unsupported_events_are_rejected_even_without_content() {
        for kind in ["content.withdrawn", "unknown"] {
            let mut value = message("正文");
            value["data"]["attributes"]["event_type"] = json!(kind);
            value["data"]["attributes"]
                .as_object_mut()
                .unwrap()
                .remove("content");
            assert_eq!(
                parse_event(&serde_json::to_vec(&value).unwrap())
                    .err()
                    .unwrap()
                    .0,
                "unsupported_event_type"
            );
        }
    }
    #[test]
    fn caller_credentials_do_not_accept_downstream_host_configuration() {
        let input = json!({"unit_name":"Linjian","valid_from":"2026-11-08T00:00:00Z","valid_until":"2027-11-08T00:00:00Z"});
        let parsed: AdminInput = serde_json::from_value(input.clone()).unwrap();
        assert!(parsed.key_id.is_none());
        for field in [
            "source_id",
            "origins",
            "path_prefix",
            "callback_url",
            "online_status_url",
        ] {
            let mut invalid = input.clone();
            invalid[field] = json!("https://linjian.example.com");
            assert!(serde_json::from_value::<AdminInput>(invalid).is_err());
        }
    }
    #[test]
    fn credential_lifetime_checks_status_start_and_optional_end() {
        let now = chrono::Utc::now();
        let before = now - chrono::Duration::seconds(1);
        let after = now + chrono::Duration::seconds(1);
        assert!(credential_available("active", now, None, now));
        assert!(credential_available("active", before, Some(after), now));
        assert!(!credential_available("active", after, None, now));
        assert!(!credential_available("active", before, Some(now), now));
        assert!(!credential_available("active", before, Some(before), now));
        for status in ["disabled", "revoked"] {
            assert!(!credential_available(status, before, None, now));
        }
    }
    #[test]
    fn perpetual_credential_accepts_null_or_omitted_expiry() {
        for value in [json!({"valid_until":null}), json!({})] {
            let parsed: AdminInput = serde_json::from_value(value).unwrap();
            assert!(parsed.valid_until.is_none());
        }
    }
    #[test]
    fn strict_envelope_forbids_execution_and_school_fields() {
        let id = Uuid::nil();
        let attributes = json!({"event_id":id,"schema_version":2,"route_id":"route-1","event_type":"content.upsert","occurred_at":"2026-11-08T00:00:00Z","content":"正文"});
        let doc = json!({"data":{"type":"source-events","id":id,"attributes":attributes}});
        let valid: Document<Event> = serde_json::from_value(doc.clone()).unwrap();
        valid.data.attributes.validate().unwrap();
        for field in [
            "school_id",
            "school_name",
            "external_organization_id",
            "channel",
            "group_ids",
            "device_id",
            "sending_account",
            "ad_id",
            "ad_pool_id",
        ] {
            let mut bad = doc.clone();
            bad["data"]["attributes"][field] = json!("injected");
            assert!(
                serde_json::from_value::<Document<Event>>(bad).is_err(),
                "{field}"
            );
        }
        let mut bad = doc.clone();
        bad["data"]["relationships"] = json!({});
        assert!(serde_json::from_value::<Document<Event>>(bad).is_err());
        let mut bad = doc;
        bad["included"] = json!([]);
        assert!(serde_json::from_value::<Document<Event>>(bad).is_err());
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementFilter {
    #[serde(rename = "page[number]")]
    pub page: Option<i64>,
    #[serde(rename = "page[size]")]
    pub page_size: Option<i64>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub key_id: Option<String>,
    /// Filter credentials that start on/before this instant and expire on/before this instant.
    pub valid_from: Option<chrono::DateTime<chrono::Utc>>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
}
impl ManagementFilter {
    pub fn pagination(&self) -> crate::business::Page {
        crate::business::Page {
            page: self.page,
            page_size: self.page_size,
            ..Default::default()
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationInput {
    pub revision: i64,
    pub status: String,
}

pub mod routing;

pub mod projection;

pub mod keyword_filter;
