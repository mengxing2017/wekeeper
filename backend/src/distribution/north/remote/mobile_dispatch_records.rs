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

use axum::{
    body::{Body, Bytes, HttpBody},
    response::{IntoResponse, Response},
};
use std::{
    pin::Pin,
    sync::{Arc, LazyLock},
    task::{Context, Poll},
};
static OBSERVERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(64)));
struct ObservedBody {
    body: Body,
    service: Service,
    id: Uuid,
    done: bool,
}
impl ObservedBody {
    fn finish(&mut self, state: &'static str) {
        if self.done {
            return;
        }
        self.done = true;
        let Ok(permit) = OBSERVERS.clone().try_acquire_owned() else {
            tracing::error!(exchange_id=%self.id,"response observer saturated; persistent record remains unknown");
            return;
        };
        let service = self.service.clone();
        let id = self.id;
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) = service.response_observed(id, state).await {
                tracing::error!(exchange_id=%id,%error,"response observation could not be persisted");
            }
        });
    }
}
impl HttpBody for ObservedBody {
    type Data = Bytes;
    type Error = axum::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<http_body::Frame<Bytes>, Self::Error>>> {
        let result = Pin::new(&mut self.body).poll_frame(cx);
        match &result {
            Poll::Ready(Some(Ok(_))) if self.body.is_end_stream() => {
                self.finish("server_write_completed")
            }
            Poll::Ready(None) => self.finish("server_write_completed"),
            Poll::Ready(Some(Err(_))) => self.finish("server_write_interrupted"),
            _ => {}
        }
        result
    }
    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
}
impl Drop for ObservedBody {
    fn drop(&mut self) {
        if !self.done {
            self.finish("response_observation_unknown");
        }
    }
}
pub async fn observe(response: ApiResponse, service: Service, id: Uuid) -> Response {
    if let Err(error) = service.response_observed(id, "response_started").await {
        tracing::warn!(exchange_id=%id,%error,"response start observation failed");
    }
    let (parts, body) = response.into_response().into_parts();
    Response::from_parts(
        parts,
        Body::new(ObservedBody {
            body,
            service,
            id,
            done: false,
        }),
    )
}
