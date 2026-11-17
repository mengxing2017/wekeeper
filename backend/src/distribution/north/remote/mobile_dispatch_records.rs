//! Read-only administration and response observation. No persistence queries at HTTP.
use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    distribution::{
        domain::shared_mobile::dispatch_records::RecordQuery,
        north::local::shared_send_v2::Service,
        south::adapter::postgres::shared_send_v2::Repository,
    },
    http::jsonapi::{ApiResponse, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
};
use uuid::Uuid;
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "intakes", None, q)
            .await?,
    ))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "detail", Some(id), q)
            .await?,
    ))
}
async fn intake_child(
    State(a): State<Application>,
    actor: Actor,
    Path((id, kind)): Path<(Uuid, String)>,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    crate::business::require(
        matches!(kind.as_str(), "events" | "operations"),
        "不支持的记录类型",
    )?;
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, &kind, Some(id), q)
            .await?,
    ))
}
async fn checks(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "checks", Some(id), q)
            .await?,
    ))
}
async fn exchanges(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "exchanges", None, q)
            .await?,
    ))
}
async fn exchange(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "exchange", Some(id), q)
            .await?,
    ))
}
async fn exchange_checks(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(q): Query<RecordQuery>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-dispatch-records",
        Service(Repository(a.pool))
            .records_read(&actor, "exchange-checks", Some(id), q)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/mobile-dispatch/intakes", get(list))
        .route("/api/v2/admin/mobile-dispatch/intakes/{id}", get(detail))
        .route(
            "/api/v2/admin/mobile-dispatch/intakes/{id}/{kind}",
            get(intake_child),
        )
        .route(
            "/api/v2/admin/mobile-dispatch/operations/{id}/checks",
            get(checks),
        )
        .route(
            "/api/v2/admin/mobile-dispatch/delivery-exchanges",
            get(exchanges),
        )
        .route(
            "/api/v2/admin/mobile-dispatch/delivery-exchanges/{id}",
            get(exchange),
        )
        .route(
            "/api/v2/admin/mobile-dispatch/delivery-exchanges/{id}/checks",
            get(exchange_checks),
        )
}
