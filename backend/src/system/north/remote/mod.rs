use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::bootstrap::Application;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
pub struct SystemDocument {
    data: SystemResponse,
}

#[derive(Serialize)]
struct SystemResponse {
    application: &'static str,
    version: &'static str,
    stage: &'static str,
}

pub async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn readiness(State(app): State<Application>) -> (StatusCode, Json<HealthResponse>) {
    if app.system.ready().await {
        (StatusCode::OK, Json(HealthResponse { status: "ready" }))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unavailable",
            }),
        )
    }
}

pub async fn system_info(State(app): State<Application>) -> Json<SystemDocument> {
    let info = app.system.info();
    Json(SystemDocument {
        data: SystemResponse {
            application: info.application,
            version: info.version,
            stage: info.stage,
        },
    })
}
