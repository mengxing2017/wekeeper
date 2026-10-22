pub mod error;
pub mod jsonapi;
use axum::{
    Router,
    http::{StatusCode, header::HeaderName},
    routing::get,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{bootstrap::Application, system::north::remote};

async fn not_found() -> axum::response::Response {
    error::response(StatusCode::NOT_FOUND, "not_found", "接口不存在")
}

pub fn app_router(application: Application) -> Router {
    let request_id = HeaderName::from_static("x-request-id");
    Router::new()
        .route("/healthz", get(remote::liveness))
        .route("/readyz", get(remote::readiness))
        .route("/api/v1/system", get(remote::system_info))
        .merge(crate::identity::north::remote::router())
.merge(crate::schools::north::remote::router())
.merge(crate::community::north::remote::router())
.merge(crate::distribution::north::remote::router())
.merge(crate::distribution::north::broadcast_remote::router())
.merge(crate::integrations::north::remote::router())
.merge(crate::integrations::north::directory_remote::router())
        .merge(crate::integrations::north::assets_remote::router())
.merge(crate::legacy::north::remote::router())
.merge(crate::media::north::remote::router())
.route("/api/v1/schools/{school}/audit",get(crate::audit::list))
        .fallback(not_found)
        .with_state(application)
        .layer(axum::middleware::from_fn(error::normalize_errors))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| tracing::info_span!("http",method=%request.method(),request_id=?request.headers().get("x-request-id"))),
        )
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}
