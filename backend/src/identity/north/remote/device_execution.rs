//! Device credentials never pass through the browser/miniapp Actor extractor.
use crate::{
    bootstrap::Application, business::Error, distribution::domain::mobile_delivery::Principal,
};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};

fn rejection(error: Error) -> Response {
    match error {
        Error::Unauthorized => crate::http::error::response(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "设备身份认证未通过，请管理员核对设备启用状态和凭据",
        ),
        error => error.into_response(),
    }
}
pub struct DevicePrincipal(pub Principal);
impl FromRequestParts<Application> for DevicePrincipal {
    type Rejection = Response;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Application,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .filter(|t| t.starts_with("wk_device_"))
            .ok_or_else(|| {
                tracing::warn!(
                    reason = "authorization_missing_or_malformed",
                    "device authentication rejected"
                );
                rejection(Error::Unauthorized)
            })?;
        let protocol = if parts.uri.path().starts_with("/api/v5/device-execution/") {
            5
        } else if parts.uri.path().starts_with("/api/v4/device-execution/") {
            4
        } else if parts.uri.path().starts_with("/api/v3/device-execution/") {
            3
        } else if parts.uri.path().starts_with("/api/v2/device-execution/") {
            2
        } else {
            1
        };
        Ok(Self(
            state
                .mobile_delivery()
                .authenticate(token)
                .await
                .map_err(rejection)?
                .for_protocol(protocol),
        ))
    }
}
