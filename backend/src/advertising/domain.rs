//! Advertising decisions and composition are independent of HTTP and persistence.
use crate::source_access::domain::routing::RouteSnapshot;
use crate::source_access::domain::{Fault, Result, digest, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub mod pool;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub kind: String,
    pub id: String,
    pub channel: Option<String>,
    pub workspace_id: Option<Uuid>,
}
impl Scope {
    pub fn matches(&self, s: &RouteSnapshot) -> bool {
        let Some(e) = &s.execution else { return false };
        let identity = match self.kind.as_str() {
            "unit" => Some(s.unit_id.to_string()),
            "group" => Some(s.group_id.to_string()),
            "route" => Some(s.route_id.clone()),
            _ => None,
        };
        identity.as_deref() == Some(&self.id)
            && self
                .channel
                .as_deref()
                .is_none_or(|c| c == e.channel.as_str())
            && self.workspace_id.is_none_or(|w| w == e.workspace_id)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub allows: Vec<Scope>,
    pub denies: Vec<Scope>,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.valid_until.is_none_or(|t| t > self.valid_from)
                && self.allows.len() + self.denies.len() <= 2000,
            "invalid_ad_policy",
        )?;
        for (allow, scopes) in [(true, &self.allows), (false, &self.denies)] {
            for s in scopes {
                ensure(
                    matches!(s.kind.as_str(), "unit" | "group" | "route")
                        && !s.id.is_empty()
                        && s.id.len() <= 128,
                    "invalid_ad_scope",
                )?;
                ensure(
                    matches!(s.channel.as_deref(), None | Some("mobile" | "wecom"))
                        && s.channel.is_some() == s.workspace_id.is_some()
                        && (!allow || s.channel.is_some()),
                    "invalid_ad_scope",
                )?;
            }
        }
        Ok(())
    }
    pub fn exclusion(
        &self,
        s: &RouteSnapshot,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<&'static str> {
        if !self.enabled {
            Some("ad_disabled")
        } else if now < self.valid_from {
            Some("ad_not_started")
        } else if self.valid_until.is_some_and(|t| now >= t) {
            Some("ad_expired")
        } else if self.denies.iter().any(|v| v.matches(s)) {
            Some("ad_denied")
        } else if !self.allows.iter().any(|v| v.matches(s)) {
            Some("ad_not_allowed")
        } else {
            None
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub name: String,
    pub body: String,
    pub link: Option<String>,
}
impl Content {
    pub fn validate(&self) -> Result<()> {
        ensure(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 120
                && !self.body.trim().is_empty()
                && self.body.len() <= 8000,
            "invalid_ad_content",
        )?;
        if let Some(link) = &self.link {
            let u = url::Url::parse(link).map_err(|_| Fault("invalid_ad_link"))?;
            ensure(
                link.len() <= 2048
                    && !link.chars().any(|c| c.is_control() || c.is_whitespace())
                    && u.scheme() == "https"
                    && u.host_str().is_some()
                    && u.username().is_empty()
                    && u.password().is_none(),
                "invalid_ad_link",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub mode: String,
    pub fixed_ad_id: Option<Uuid>,
    pub insertion_position: String,
    pub unavailable_policy: String,
    pub auto_recover: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "none".into(),
            fixed_ad_id: None,
            insertion_position: "after".into(),
            unavailable_policy: "without_ad".into(),
            auto_recover: true,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure(
            matches!(self.mode.as_str(), "none" | "fixed" | "pool")
                && (self.mode == "fixed") == self.fixed_ad_id.is_some()
                && matches!(self.insertion_position.as_str(), "before" | "after")
                && matches!(
                    self.unavailable_policy.as_str(),
                    "without_ad" | "configuration_error"
                ),
            "invalid_ad_configuration",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub bytes: usize,
    pub scalars: usize,
    pub max_bytes: usize,
    pub max_scalars: Option<usize>,
    pub valid: bool,
}
pub fn measure(text: &str, channel: &str) -> Measurement {
    let bytes = text.len();
    let scalars = text.chars().count();
    let (max_bytes, max_scalars) = if channel == "wecom" {
        (4000, None)
    } else {
        (8000, Some(2000))
    };
    Measurement {
        bytes,
        scalars,
        max_bytes,
        max_scalars,
        valid: !text.trim().is_empty()
            && bytes <= max_bytes
            && max_scalars.is_none_or(|v| scalars <= v)
            && !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
    }
}
pub fn compose(source: &str, ad: Option<&Content>, position: &str) -> String {
    let out = if let Some(ad) = ad {
        let block = format!(
            "【广告】\n{}{}",
            ad.body,
            ad.link
                .as_ref()
                .map(|v| format!("\n{v}"))
                .unwrap_or_default()
        );
        if position == "before" {
            format!("{block}\n\n{source}")
        } else {
            format!("{source}\n\n{block}")
        }
    } else {
        source.to_owned()
    };
    out.replace("\r\n", "\n").replace('\r', "\n")
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateEvidence {
    pub ad_id: Uuid,
    pub content_version: Option<Uuid>,
    pub policy_version: Option<Uuid>,
    pub exclusion: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<pool::Selection>,
    pub candidates: Vec<CandidateEvidence>,
    pub config: Config,
    pub config_revision: i64,
    pub ad_id: Option<Uuid>,
    pub content_version: Option<Uuid>,
    pub policy_version: Option<Uuid>,
    pub composer: String,
    pub exclusions: Vec<String>,
    pub measurement: Measurement,
}
impl Selection {
    pub fn unavailable(&self) -> bool {
        matches!(self.config.mode.as_str(), "fixed" | "pool")
            && self.ad_id.is_none()
            && self.config.unavailable_policy == "configuration_error"
    }
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub selection: Selection,
    pub content: String,
    pub base: Option<Uuid>,
    pub replacement: bool,
}
impl Prepared {
    pub fn digest(&self) -> String {
        digest(self.content.as_bytes())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    pub owner_unit_id: Uuid,
    pub content: Content,
    pub workspaces: Vec<Scope>,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentPublication {
    pub expected_revision: i64,
    pub reason: String,
    pub content: Content,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyPublication {
    pub expected_revision: i64,
    pub reason: String,
    pub policy: Policy,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub expected_revision: i64,
    pub reason: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub expected_revision: i64,
    pub reason: String,
    pub config: Config,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manager {
    pub username: String,
    pub expected_revision: i64,
    pub can_edit: bool,
    pub can_share: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub title: Option<String>,
    pub content: Option<String>,
    pub detail_url: String,
}
pub enum Command {
    Directory,
    List { page: i64, filter: String },
    Create(Create),
    Detail(Uuid),
    Versions(Uuid, i64),
    Content(Uuid, ContentPublication),
    Policy(Uuid, PolicyPublication),
    Archive(Uuid, Revision),
    Managers(Uuid),
    Manager(Uuid, Manager),
    Candidates(String, i64),
    Config(String),
    Configure(String, Configuration),
    Preview(String, Preview),
    Compositions(String, Uuid, i64),
    Pool(String),
    PublishPool(String, pool::Publication),
    ResetPool(String, pool::Reset),
    CopyPool(String, pool::Copy),
    Consumptions(String, i64),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_and_scalar_byte_limits_are_exact() {
        let c = Content {
            name: "名称".into(),
            body: "优惠\r\n第二行\r第三行".into(),
            link: Some("https://example.com/ad".into()),
        };
        let source = "标题\n正文\nhttps://example.com/source";
        assert_eq!(
            compose(source, Some(&c), "before"),
            "【广告】\n优惠\n第二行\n第三行\nhttps://example.com/ad\n\n标题\n正文\nhttps://example.com/source"
        );
        assert_eq!(
            compose(source, Some(&c), "after"),
            format!("{source}\n\n【广告】\n优惠\n第二行\n第三行\nhttps://example.com/ad")
        );
        assert!(measure(&"😀".repeat(2000), "mobile").valid);
        assert!(!measure(&"😀".repeat(2001), "mobile").valid);
        assert!(measure(&"a".repeat(4000), "wecom").valid);
        assert!(!measure(&"a".repeat(4001), "wecom").valid);
        assert!(!measure("正文\u{0000}", "mobile").valid);
        assert!(measure("正文\t\n", "mobile").valid);
    }
    #[test]
    fn fixture_is_serialized_by_real_rust_types_and_accepts_pool_mode() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/source-advertising.json"
        ))
        .unwrap();
        for key in [
            "create",
            "configuration",
            "policy_publication",
            "content_publication",
        ] {
            let value = fixture[key].clone();
            let actual = match key {
                "create" => {
                    serde_json::to_value(serde_json::from_value::<Create>(value.clone()).unwrap())
                        .unwrap()
                }
                "configuration" => serde_json::to_value(
                    serde_json::from_value::<Configuration>(value.clone()).unwrap(),
                )
                .unwrap(),
                "policy_publication" => serde_json::to_value(
                    serde_json::from_value::<PolicyPublication>(value.clone()).unwrap(),
                )
                .unwrap(),
                _ => serde_json::to_value(
                    serde_json::from_value::<ContentPublication>(value.clone()).unwrap(),
                )
                .unwrap(),
            };
            assert_eq!(actual, value);
        }
        let selection: Selection = serde_json::from_value(fixture["selection"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(selection).unwrap(),
            fixture["selection"]
        );
        let mut config: Config =
            serde_json::from_value(fixture["configuration"]["config"].clone()).unwrap();
        config.mode = "pool".into();
        config.fixed_ad_id = None;
        assert!(config.validate().is_ok());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"mode":"none","fixed_ad_id":null,"insertion_position":"after","unavailable_policy":"without_ad","auto_recover":true,"pool_cursor":1})).is_err());
    }
    #[test]
    fn pool_publication_reset_copy_and_config_use_real_wire_types() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/source-advertising.json"
        ))
        .unwrap();
        let publication: pool::Publication =
            serde_json::from_value(fixture["pool_publication"].clone()).unwrap();
        publication.validate().unwrap();
        assert_eq!(
            serde_json::to_value(publication).unwrap(),
            fixture["pool_publication"]
        );
        let selection: Selection =
            serde_json::from_value(fixture["pool_selection"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(selection).unwrap(),
            fixture["pool_selection"]
        );
        let reset: pool::Reset = serde_json::from_value(fixture["pool_reset"].clone()).unwrap();
        assert_eq!(serde_json::to_value(reset).unwrap(), fixture["pool_reset"]);
        let copy: pool::Copy = serde_json::from_value(fixture["pool_copy"].clone()).unwrap();
        assert_eq!(serde_json::to_value(copy).unwrap(), fixture["pool_copy"]);
        let config: Configuration =
            serde_json::from_value(fixture["pool_configuration"].clone()).unwrap();
        config.config.validate().unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            fixture["pool_configuration"]
        );
    }
    #[test]
    fn authorization_is_private_exact_scoped_and_deny_wins() {
        let unit = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let mut s = RouteSnapshot {
            route_id: "route".into(),
            revision: 1,
            source_id: Uuid::new_v4(),
            unit_id: unit,
            unit_name: "单位".into(),
            group_id: Uuid::new_v4(),
            group_name: "分组".into(),
            execution: Some(crate::source_access::domain::routing::ExecutionConfig {
                channel: crate::source_access::domain::routing::Channel::Mobile,
                workspace_id: workspace,
                target_ids: vec![],
            }),
            targets: vec![],
            configured_by: Uuid::new_v4(),
        };
        let now = chrono::Utc::now();
        let mut p = Policy {
            enabled: true,
            valid_from: now,
            valid_until: None,
            allows: vec![Scope {
                kind: "unit".into(),
                id: unit.to_string(),
                channel: Some("mobile".into()),
                workspace_id: Some(workspace),
            }],
            denies: vec![],
        };
        assert!(p.validate().is_ok());
        assert!(p.exclusion(&s, now).is_none());
        s.unit_id = Uuid::new_v4();
        assert_eq!(p.exclusion(&s, now), Some("ad_not_allowed"));
        s.unit_id = unit;
        p.denies.push(Scope {
            kind: "group".into(),
            id: s.group_id.to_string(),
            channel: None,
            workspace_id: None,
        });
        assert_eq!(p.exclusion(&s, now), Some("ad_denied"));
        p.denies.clear();
        p.valid_until = Some(now);
        assert_eq!(p.exclusion(&s, now), Some("ad_expired"));
    }
}
