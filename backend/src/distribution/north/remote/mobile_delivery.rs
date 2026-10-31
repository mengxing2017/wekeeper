//! HTTP adapter: wire conversion only; all authorization and SQL stay behind the store.
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Page, Result},
    distribution::domain::mobile_delivery::{Command, Principal, Query as Read, Scope},
    http::jsonapi::{Input, collection, completed, resource},
    identity::north::remote::device_execution::DevicePrincipal,
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use uuid::Uuid;
mod automation;
mod dispatch;
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
        .mobile_delivery()
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
        .mobile_delivery()
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
        .mobile_delivery()
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
        .mobile_delivery()
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
        .mobile_delivery()
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
        .mobile_delivery()
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
async fn accounts(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            Read::Accounts,
            page.clone(),
        )
        .await?;
    Ok(response("sending-accounts", value, Some(&page), false))
}
async fn create_account(
    State(a): State<Application>,
    actor: Actor,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "sending-accounts", "create_account")?,
        )
        .await?;
    Ok(response("sending-accounts", value, None, true))
}
async fn responsible_actors(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            Read::ResponsibleActors,
            page.clone(),
        )
        .await?;
    Ok(response(
        "sending-account-responsible-actors",
        value,
        Some(&page),
        false,
    ))
}
async fn bind_device(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "device-account-bindings", "bind_device")?,
        )
        .await?;
    Ok(response("device-account-bindings", value, None, false))
}
async fn account_policy(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            Read::AccountPolicy,
            page.clone(),
        )
        .await?;
    Ok(response("sending-accounts", value, Some(&page), false))
}
async fn update_account_policy(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "sending-accounts", "account_policy")?,
        )
        .await?;
    Ok(response("sending-accounts", value, None, false))
}
async fn groups(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Groups,
            page.clone(),
        )
        .await?;
    Ok(response("delivery-groups", value, Some(&page), false))
}
async fn create_group(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            key(&headers),
            command(input, "delivery-groups", "create_group")?,
        )
        .await?;
    Ok(response("delivery-groups", value, None, true))
}
async fn bind_group(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "delivery-groups", "bind_group")?,
        )
        .await?;
    Ok(response("delivery-groups", value, None, false))
}
async fn group_policy(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::GroupPolicy,
            page.clone(),
        )
        .await?;
    Ok(response("delivery-groups", value, Some(&page), false))
}
async fn update_group_policy(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "delivery-groups", "group_policy")?,
        )
        .await?;
    Ok(response("delivery-groups", value, None, false))
}
async fn capabilities(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Capabilities,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-capabilities",
        value,
        Some(&page),
        false,
    ))
}
async fn settings(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Settings,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-settings",
        value,
        Some(&page),
        false,
    ))
}
async fn update_settings(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            key(&headers),
            command(input, "mobile-delivery-settings", "settings")?,
        )
        .await?;
    Ok(response("mobile-delivery-settings", value, None, false))
}
async fn tasks(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Tasks,
            page.clone(),
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, Some(&page), false))
}
async fn create_task(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            key(&headers),
            command(input, "mobile-delivery-tasks", "draft")?,
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, None, true))
}
async fn task(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::Task,
            page.clone(),
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, Some(&page), false))
}
async fn edit_task(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-tasks", "draft")?,
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, None, false))
}
async fn preview(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-previews", "preview")?,
        )
        .await?;
    Ok(response("mobile-delivery-previews", value, None, false))
}
async fn confirm(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-tasks", "confirm")?,
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, None, false))
}
async fn stop(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-tasks", "stop")?,
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, None, false))
}
async fn items(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::Items,
            page.clone(),
        )
        .await?;
    Ok(response("mobile-delivery-items", value, Some(&page), false))
}
async fn events(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::Events,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-events",
        value,
        Some(&page),
        false,
    ))
}
async fn reconcile(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-items", "reconcile")?,
        )
        .await?;
    Ok(response("mobile-delivery-items", value, None, false))
}
async fn resend(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-tasks", "resend")?,
        )
        .await?;
    Ok(response("mobile-delivery-tasks", value, None, true))
}
async fn enroll(
    State(a): State<Application>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Enrollment.for_protocol(if uri.path().starts_with("/api/v5/") {
                5
            } else if uri.path().starts_with("/api/v4/") {
                4
            } else if uri.path().starts_with("/api/v3/") {
                3
            } else if uri.path().starts_with("/api/v2/") {
                2
            } else {
                1
            }),
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
    DevicePrincipal(principal): DevicePrincipal,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
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
async fn session(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            principal,
            Scope {
                school: None,
                id: None,
            },
            Read::Session,
            page.clone(),
        )
        .await?;
    Ok(response(
        "device-execution-sessions",
        value,
        Some(&page),
        false,
    ))
}
async fn heartbeat(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "device-heartbeats", "heartbeat")?,
        )
        .await?;
    Ok(response("device-heartbeats", value, None, false))
}
async fn claim(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "mobile-execution-claims", "claim")?,
        )
        .await?;
    Ok(response("mobile-execution-claims", value, None, false))
}
async fn get_claim(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            Read::Claim,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-execution-claims",
        value,
        Some(&page),
        false,
    ))
}
async fn renew(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-execution-claims", "renew")?,
        )
        .await?;
    Ok(response("mobile-execution-claims", value, None, false))
}
async fn prepare(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-execution-preparations", "prepare")?,
        )
        .await?;
    Ok(response(
        "mobile-execution-preparations",
        value,
        None,
        false,
    ))
}
async fn permit(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-send-permits", "permit")?,
        )
        .await?;
    Ok(response("mobile-send-permits", value, None, false))
}
async fn result(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-items", "result")?,
        )
        .await?;
    Ok(response("mobile-delivery-items", value, None, false))
}
async fn abort(
    State(a): State<Application>,
    DevicePrincipal(principal): DevicePrincipal,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            principal,
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-items", "abort")?,
        )
        .await?;
    Ok(response("mobile-delivery-items", value, None, false))
}
async fn update_account_profile(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "sending-accounts", "update_account_profile")?,
        )
        .await?;
    Ok(response("sending-accounts", value, None, false))
}

async fn review_account_profile(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "sending-accounts", "review_account_profile")?,
        )
        .await?;
    Ok(response("sending-accounts", value, None, false))
}

async fn update_group_profile(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "delivery-groups", "update_group_profile")?,
        )
        .await?;
    Ok(response("delivery-groups", value, None, false))
}

async fn review_group_profile(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "delivery-groups", "review_group_profile")?,
        )
        .await?;
    Ok(response("delivery-groups", value, None, false))
}

async fn create_strategy(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            key(&headers),
            command(input, "mobile-delivery-strategies", "create_strategy")?,
        )
        .await?;
    Ok(response("mobile-delivery-strategies", value, None, true))
}

async fn update_strategy(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-strategies", "update_strategy")?,
        )
        .await?;
    Ok(response("mobile-delivery-strategies", value, None, false))
}

async fn run_strategy(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-strategy-runs", "run_strategy")?,
        )
        .await?;
    Ok(response("mobile-delivery-strategy-runs", value, None, true))
}

async fn strategies(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Strategies,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-strategies",
        value,
        Some(&page),
        false,
    ))
}

async fn strategy(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::Strategy,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-strategies",
        value,
        Some(&page),
        false,
    ))
}

async fn strategy_runs(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::StrategyRuns,
            page.clone(),
        )
        .await?;
    Ok(response(
        "mobile-delivery-strategy-runs",
        value,
        Some(&page),
        false,
    ))
}

async fn visible_targets(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let value = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::Targets,
            page,
        )
        .await?;
    Ok(response("mobile-delivery-targets", value, None, false))
}
async fn configurations(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            Read::Configurations,
            page.clone(),
        )
        .await?;
    Ok(response("execution-configurations", v, Some(&page), false))
}
async fn create_configuration(
    State(a): State<Application>,
    actor: Actor,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "execution-configurations", "create_configuration")?,
        )
        .await?;
    Ok(response("execution-configurations", v, None, true))
}
async fn configuration(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            Read::Configuration,
            page,
        )
        .await?;
    Ok(response("execution-configurations", v, None, false))
}
async fn publish_configuration(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(input, "configuration-releases", "publish_configuration")?,
        )
        .await?;
    Ok(response("configuration-releases", v, None, true))
}
async fn releases(
    State(a): State<Application>,
    actor: Actor,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: None,
            },
            Read::ConfigurationReleases,
            page.clone(),
        )
        .await?;
    Ok(response("configuration-releases", v, Some(&page), false))
}
async fn school_releases(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: None,
            },
            Read::ConfigurationReleases,
            page.clone(),
        )
        .await?;
    Ok(response("configuration-releases", v, Some(&page), false))
}
async fn revoke_release(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: None,
                id: Some(id),
            },
            key(&headers),
            command(
                input,
                "configuration-releases",
                "revoke_configuration_release",
            )?,
        )
        .await?;
    Ok(response("configuration-releases", v, None, false))
}
async fn resolve_configuration(
    State(a): State<Application>,
    DevicePrincipal(p): DevicePrincipal,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            p,
            Scope {
                school: None,
                id: None,
            },
            key(&headers),
            command(input, "configuration-releases", "resolve_configuration")?,
        )
        .await?;
    Ok(response("configuration-releases", v, None, false))
}
async fn download_configuration(
    State(a): State<Application>,
    DevicePrincipal(p): DevicePrincipal,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            p,
            Scope {
                school: None,
                id: Some(id),
            },
            Read::ConfigurationDownload,
            page,
        )
        .await?;
    Ok(response("configuration-releases", v, None, false))
}
async fn run_progress(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            Read::RunProgress,
            page,
        )
        .await?;
    Ok(response("mobile-delivery-run-progress", v, None, false))
}
async fn revision_preview(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(
                input,
                "mobile-delivery-run-progress",
                "preview_run_revision",
            )?,
        )
        .await?;
    Ok(response("mobile-delivery-run-progress", v, None, false))
}
async fn revise_run(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    input: Input,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .execute(
            Principal::Actor(actor.id),
            Scope {
                school: Some(school),
                id: Some(id),
            },
            key(&headers),
            command(input, "mobile-delivery-run-progress", "revise_run")?,
        )
        .await?;
    Ok(response("mobile-delivery-run-progress", v, None, false))
}

pub fn router() -> Router<Application> {
    Router::new()
        .merge(automation::router())
        .merge(dispatch::router())
        .route(
            "/api/v1/platform/execution-configurations",
            get(configurations).post(create_configuration),
        )
        .route(
            "/api/v1/platform/execution-configurations/{id}",
            get(configuration),
        )
        .route(
            "/api/v1/platform/execution-configurations/{id}/releases",
            post(publish_configuration),
        )
        .route("/api/v1/platform/configuration-releases", get(releases))
        .route(
            "/api/v1/platform/configuration-releases/{id}/revocations",
            post(revoke_release),
        )
        .route(
            "/api/v1/schools/{school}/configuration-releases",
            get(school_releases),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-runs/{id}",
            get(run_progress),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-runs/{id}/revision-previews",
            post(revision_preview),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-runs/{id}/execution-revisions",
            post(revise_run),
        )
        .route(
            "/api/v3/device-execution/configuration-resolutions",
            post(resolve_configuration),
        )
        .route(
            "/api/v3/device-execution/configuration-releases/{id}",
            get(download_configuration),
        )
        .route("/api/v5/device-execution/enrollments", post(enroll))
        .route(
            "/api/v5/device-execution/credential-rotations",
            post(rotate),
        )
        .route("/api/v5/device-execution/session", get(session))
        .route("/api/v5/device-execution/claims", post(claim))
        .route("/api/v5/device-execution/claims/{id}", get(get_claim))
        .route("/api/v5/device-execution/claims/{id}/renewals", post(renew))
        .route("/api/v5/device-execution/claims/{id}/permits", post(permit))
        .route("/api/v5/device-execution/claims/{id}/results", post(result))
        .route("/api/v5/device-execution/claims/{id}/aborts", post(abort))
        .route("/api/v4/device-execution/enrollments", post(enroll))
        .route(
            "/api/v4/device-execution/credential-rotations",
            post(rotate),
        )
        .route("/api/v4/device-execution/session", get(session))
        .route("/api/v4/device-execution/heartbeats", post(heartbeat))
        .route("/api/v4/device-execution/claims", post(claim))
        .route("/api/v4/device-execution/claims/{id}", get(get_claim))
        .route("/api/v4/device-execution/claims/{id}/renewals", post(renew))
        .route(
            "/api/v4/device-execution/claims/{id}/preparations",
            post(prepare),
        )
        .route("/api/v4/device-execution/claims/{id}/permits", post(permit))
        .route("/api/v4/device-execution/claims/{id}/results", post(result))
        .route("/api/v4/device-execution/claims/{id}/aborts", post(abort))
        .route("/api/v3/device-execution/enrollments", post(enroll))
        .route(
            "/api/v3/device-execution/credential-rotations",
            post(rotate),
        )
        .route("/api/v3/device-execution/session", get(session))
        .route("/api/v3/device-execution/heartbeats", post(heartbeat))
        .route("/api/v3/device-execution/claims", post(claim))
        .route("/api/v3/device-execution/claims/{id}", get(get_claim))
        .route("/api/v3/device-execution/claims/{id}/renewals", post(renew))
        .route(
            "/api/v3/device-execution/claims/{id}/preparations",
            post(prepare),
        )
        .route("/api/v3/device-execution/claims/{id}/permits", post(permit))
        .route("/api/v3/device-execution/claims/{id}/results", post(result))
        .route("/api/v3/device-execution/claims/{id}/aborts", post(abort))
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
        .route(
            "/api/v1/platform/sending-accounts",
            get(accounts).post(create_account),
        )
        .route(
            "/api/v1/platform/sending-account-responsible-actors",
            get(responsible_actors),
        )
        .route(
            "/api/v1/platform/execution-devices/{id}/account-bindings",
            post(bind_device),
        )
        .route(
            "/api/v1/platform/sending-accounts/{id}/delivery-policy",
            get(account_policy).patch(update_account_policy),
        )
        .route(
            "/api/v1/schools/{school}/delivery-groups",
            get(groups).post(create_group),
        )
        .route(
            "/api/v1/schools/{school}/delivery-groups/{id}/sender-bindings",
            post(bind_group),
        )
        .route(
            "/api/v1/schools/{school}/delivery-groups/{id}/delivery-policy",
            get(group_policy).patch(update_group_policy),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-capabilities",
            get(capabilities),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-settings",
            get(settings).patch(update_settings),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks",
            get(tasks).post(create_task),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}",
            get(task).patch(edit_task),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}/previews",
            post(preview),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}/confirmations",
            post(confirm),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}/stops",
            post(stop),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}/items",
            get(items),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-tasks/{id}/events",
            get(events),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-items/{id}/reconciliations",
            post(reconcile),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-items/{id}/resend-drafts",
            post(resend),
        )
        .route("/api/v1/device-execution/enrollments", post(enroll))
        .route(
            "/api/v1/device-execution/credential-rotations",
            post(rotate),
        )
        .route("/api/v1/device-execution/session", get(session))
        .route("/api/v1/device-execution/heartbeats", post(heartbeat))
        .route("/api/v1/device-execution/claims", post(claim))
        .route("/api/v1/device-execution/claims/{id}", get(get_claim))
        .route("/api/v1/device-execution/claims/{id}/renewals", post(renew))
        .route(
            "/api/v1/device-execution/claims/{id}/preparations",
            post(prepare),
        )
        .route("/api/v1/device-execution/claims/{id}/permits", post(permit))
        .route("/api/v1/device-execution/claims/{id}/results", post(result))
        .route("/api/v1/device-execution/claims/{id}/aborts", post(abort))
        .route(
            "/api/v1/schools/{school}/mobile-delivery-targets",
            get(visible_targets),
        )
        .route(
            "/api/v1/platform/sending-accounts/{id}/visible-profile",
            axum::routing::patch(update_account_profile),
        )
        .route(
            "/api/v1/platform/sending-accounts/{id}/visible-profile-reviews",
            post(review_account_profile),
        )
        .route(
            "/api/v1/schools/{school}/delivery-groups/{id}/visible-profile",
            axum::routing::patch(update_group_profile),
        )
        .route(
            "/api/v1/schools/{school}/delivery-groups/{id}/visible-profile-reviews",
            post(review_group_profile),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-strategies",
            get(strategies).post(create_strategy),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-strategies/{id}",
            get(strategy).patch(update_strategy),
        )
        .route(
            "/api/v1/schools/{school}/mobile-delivery-strategies/{id}/runs",
            get(strategy_runs).post(run_strategy),
        )
        .route("/api/v2/device-execution/enrollments", post(enroll))
        .route(
            "/api/v2/device-execution/credential-rotations",
            post(rotate),
        )
        .route("/api/v2/device-execution/session", get(session))
        .route("/api/v2/device-execution/heartbeats", post(heartbeat))
        .route("/api/v2/device-execution/claims", post(claim))
        .route("/api/v2/device-execution/claims/{id}", get(get_claim))
        .route("/api/v2/device-execution/claims/{id}/renewals", post(renew))
        .route(
            "/api/v2/device-execution/claims/{id}/preparations",
            post(prepare),
        )
        .route("/api/v2/device-execution/claims/{id}/permits", post(permit))
        .route("/api/v2/device-execution/claims/{id}/results", post(result))
        .route("/api/v2/device-execution/claims/{id}/aborts", post(abort))
        .layer(DefaultBodyLimit::max(512 * 1024))
}
