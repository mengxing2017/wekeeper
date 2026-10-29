//! Device credentials never pass through the browser/miniapp Actor extractor.
use crate::{
    bootstrap::Application,
    business::{Error, Result},
    distribution::domain::mobile_delivery::Principal,
};
use axum::{extract::FromRequestParts, http::request::Parts};
pub struct DevicePrincipal(pub Principal);
impl FromRequestParts<Application> for DevicePrincipal {
    type Rejection = Error;
    async fn from_request_parts(parts: &mut Parts, state: &Application) -> Result<Self> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .filter(|t| t.starts_with("wk_device_"))
            .ok_or(Error::Unauthorized)?;
        let protocol = if parts.uri.path().starts_with("/api/v2/device-execution/") {
            2
        } else {
            1
        };
        Ok(Self(
            state
                .mobile_delivery()
                .authenticate(token)
                .await?
                .for_protocol(protocol),
        ))
    }
}
