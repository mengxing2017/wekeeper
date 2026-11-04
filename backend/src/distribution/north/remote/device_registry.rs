//! HTTP adapter: wire conversion only; all authorization and SQL stay behind the store.
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Page, Result},
    distribution::domain::device_registry::{Command, Principal, Query as Read, Scope},
    http::jsonapi::{Input, collection, completed, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use uuid::Uuid;
fn command(input: Input, kind: &str, operation: &str) -> Result<Command> {
    let mut v = input.decode::<Value>(kind)?;
    let obj = v
        .as_object_mut()
        .ok_or(Error::Invalid("attributes 必须为对象".into()))?;
    if obj.contains_key("operation") {
        return Err(Error::Invalid("不接受 operation 字段".into()));
    }
    obj.insert("operation".into(), json!(operation));
    serde_json::from_value(v).map_err(|_| Error::Invalid("请求字段无效或包含未知字段".into()))
}
fn key(headers: &HeaderMap) -> &str {
    headers
        .get("idempotency-key")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
}
fn response(kind: &str, value: Value, page: Option<&Page>, created: bool) -> Response {
    let result = if value["waiting"] == true {
        completed().meta(value)
    } else if let (Value::Array(rows), Some(page)) = (&value, page) {
        collection(kind, rows.clone(), page)
    } else {
        resource(kind, value)
    };
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        result,
    )
        .into_response()
}
async fn devices(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .device_registry()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            Read::Devices,
            page.clone(),
        )
        .await?;
    Ok(response("execution-devices", value, Some(&page), false))
}
async fn create_device(
    State(a): State<Application>,
    actor: Actor,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .device_registry()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "execution-devices", "create_device")?,
        )
        .await?;
    Ok(response("execution-devices", value, None, true))
}
async fn device(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .device_registry()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            Read::Device,
            page.clone(),
        )
        .await?;
    Ok(response("execution-devices", value, Some(&page), false))
}
async fn update_device(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .device_registry()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "execution-devices", "update_device")?,
        )
        .await?;
    Ok(response("execution-devices", value, None, false))
}
async fn enrollment(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .device_registry()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "device-enrollments", "enrollment")?,
        )
        .await?;
    Ok(response("device-enrollments", value, None, false))
}
async fn revoke(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .device_registry()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "execution-devices", "revoke")?,
        )
        .await?;
    Ok(response("execution-devices", value, None, false))
}
async fn enroll(
    State(a): State<Application>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .device_registry()
        .execute(
            Principal::Enrollment,
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "device-credentials", "enroll")?,
        )
        .await?;
    Ok(response("device-credentials", value, None, false))
}
async fn rotate(
    State(a): State<Application>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(Error::Unauthorized)?;
    let c = a.control_credential(token).await?;
    let value = a
        .device_registry()
        .execute(
            Principal::Device {
                id: c.device_id,
                generation: c.generation,
            },
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "device-credentials", "rotate")?,
        )
        .await?;
    Ok(response("device-credentials", value, None, false))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/platform/execution-devices",
            get(devices).post(create_device),
        )
        .route(
            "/api/v1/platform/execution-devices/{id}",
            get(device).patch(update_device),
        )
        .route(
            "/api/v1/platform/execution-devices/{id}/enrollments",
            post(enrollment),
        )
        .route(
            "/api/v1/platform/execution-devices/{id}/credential-revocations",
            post(revoke),
        )
        .route("/api/v9/device-execution/enrollments", post(enroll))
        .route(
            "/api/v9/device-execution/credential-rotations",
            post(rotate),
        )
}
