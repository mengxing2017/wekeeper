//! Single-ad composition and direct scope binding.
use serde::{Deserialize, Serialize};
pub mod decision;
pub mod targeting;
#[derive(Debug, PartialEq, Eq)]
pub struct AdvertisingError(pub &'static str);
pub type Result<T> = std::result::Result<T, AdvertisingError>;
fn ensure(ok: bool, code: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(AdvertisingError(code))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
                && self.body.chars().count() <= 140,
            "invalid_ad_content",
        )?;
        if let Some(link) = &self.link {
            let u = url::Url::parse(link).map_err(|_| AdvertisingError("invalid_ad_link"))?;
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
