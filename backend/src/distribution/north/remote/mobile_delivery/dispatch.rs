//! HTTP is only typed wire conversion. SQL and use-case authorization stay in the repository.
use super::*;
use crate::distribution::domain::mobile_delivery::{DispatchCommand, DispatchQuery};
fn decode(input: Input, kind: &str, action: &str) -> Result<Command> {
    let mut attributes = input.decode::<Value>(kind)?;
    let object = attributes
        .as_object_mut()
        .ok_or(Error::Invalid("attributes 必须为对象".into()))?;
    if object.contains_key("action") || object.contains_key("operation") {
        return Err(Error::Invalid("不接受 action 或 operation 字段".into()));
    }
    object.insert("action".into(), json!(action));
    let input = serde_json::from_value::<DispatchCommand>(attributes)
        .map_err(|_| Error::Invalid("字段无效或包含未知配置".into()))?;
    Ok(Command::Dispatch {
        input: Box::new(input),
    })
}
macro_rules! school_read {
    ($fn:ident,$kind:literal,$query:ident) => {
        async fn $fn(
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
                    Read::Dispatch(DispatchQuery::$query),
                    page.clone(),
                )
                .await?;
            Ok(response($kind, v, Some(&page), false))
        }
    };
}
macro_rules! school_item_read {
    ($fn:ident,$kind:literal,$query:ident) => {
        async fn $fn(
            State(a): State<Application>,
            actor: Actor,
            Path((school, id)): Path<(Uuid, Uuid)>,
        ) -> Result<Response> {
            let v = a
                .mobile_delivery()
                .query(
                    Principal::Actor(actor.id),
                    Scope {
                        school: Some(school),
                        id: Some(id),
                    },
                    Read::Dispatch(DispatchQuery::$query),
                    Page::default(),
                )
                .await?;
            Ok(response($kind, v, None, false))
        }
    };
}
macro_rules! school_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
            State(a): State<Application>,
            actor: Actor,
            Path(school): Path<Uuid>,
            headers: HeaderMap,
            input: Input,
        ) -> Result<Response> {
            let v = a
                .mobile_delivery()
                .execute(
                    Principal::Actor(actor.id),
                    Scope {
                        school: Some(school),
                        id: None,
                    },
                    key(&headers),
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, true))
        }
    };
}
macro_rules! school_item_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
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
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, false))
        }
    };
}
macro_rules! platform_read {
    ($fn:ident,$kind:literal,$query:ident) => {
        async fn $fn(
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
                    Read::Dispatch(DispatchQuery::$query),
                    page.clone(),
                )
                .await?;
            Ok(response($kind, v, Some(&page), false))
        }
    };
}
macro_rules! platform_item_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
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
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, false))
        }
    };
}
macro_rules! device_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
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
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, false))
        }
    };
}
macro_rules! device_item_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
            State(a): State<Application>,
            DevicePrincipal(p): DevicePrincipal,
            Path(id): Path<Uuid>,
            headers: HeaderMap,
            input: Input,
        ) -> Result<Response> {
            let v = a
                .mobile_delivery()
                .execute(
                    p,
                    Scope {
                        school: None,
                        id: Some(id),
                    },
                    key(&headers),
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, false))
        }
    };
}

macro_rules! platform_write {
    ($fn:ident,$kind:literal,$action:literal) => {
        async fn $fn(
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
                    decode(input, $kind, $action)?,
                )
                .await?;
            Ok(response($kind, v, None, true))
        }
    };
}
macro_rules! device_read {
    ($fn:ident,$kind:literal,$query:ident) => {
        async fn $fn(
            State(a): State<Application>,
            DevicePrincipal(p): DevicePrincipal,
            Query(page): Query<Page>,
        ) -> Result<Response> {
            let v = a
                .mobile_delivery()
                .query(
                    p,
                    Scope {
                        school: None,
                        id: None,
                    },
                    Read::Dispatch(DispatchQuery::$query),
                    page.clone(),
                )
                .await?;
            Ok(response($kind, v, Some(&page), false))
        }
    };
}
platform_read!(flows, "automation-flows", Flows);
platform_write!(create_flow, "automation-flows", "create_flow");
platform_read!(pacing, "automation-pacing-profiles", Pacing);
platform_write!(create_pacing, "automation-pacing-profiles", "create_pacing");
platform_read!(releases, "automation-flow-releases", Releases);
platform_item_write!(publish, "automation-flow-releases", "publish_flow");
platform_item_write!(revoke, "automation-flow-releases", "revoke_flow");
platform_read!(failures, "automation-execution-failures", Failures);
school_read!(school_releases, "automation-flow-releases", Releases);
school_read!(assignments, "automation-execution-assignments", Assignments);
school_read!(groups, "automation-device-groups", DeviceGroups);
school_read!(batches, "automation-dispatch-batches", Batches);
school_read!(school_failures, "automation-execution-failures", Failures);
school_item_read!(batch, "automation-dispatch-batches", Batch);
school_write!(assign, "automation-execution-assignments", "assign");
school_write!(save_group, "automation-device-groups", "save_group");
school_write!(preview, "automation-dispatch-previews", "preview");
school_write!(enable, "automation-dispatch-settings", "set_enabled");
school_item_write!(confirm, "automation-dispatch-batches", "confirm");
school_item_write!(control, "automation-dispatch-batches", "control_batch");
school_item_write!(rebuild, "automation-dispatch-batches", "rebuild_remaining");
device_read!(inbox, "automation-task-deliveries", Inbox);
device_read!(
    authorization_context,
    "automation-business-authorizations",
    AuthorizationContext
);
device_write!(
    authorization,
    "automation-business-authorizations",
    "authorization"
);
device_write!(heartbeat, "device-heartbeats", "heartbeat");
device_item_write!(received, "automation-task-deliveries", "receipt");
device_item_write!(
    lease,
    "automation-dispatch-preparations",
    "preparation_lease"
);
device_item_write!(stage, "automation-dispatch-preparations", "stage");
device_item_write!(failure, "automation-execution-failures", "failure");
device_item_write!(diagnostic, "automation-dispatch-diagnostics", "diagnostic");
device_item_write!(prepare, "mobile-execution-preparations", "prepare");
school_read!(validations, "automation-flow-validations", Validations);
school_write!(
    request_validation,
    "automation-flow-validations",
    "request_validation"
);
school_item_write!(
    cancel_validation,
    "automation-flow-validations",
    "cancel_validation"
);
device_read!(
    device_validations,
    "automation-flow-validations",
    DeviceValidations
);
device_item_write!(
    accept_validation,
    "automation-flow-validations",
    "accept_validation"
);
device_item_write!(
    validation_event,
    "automation-flow-validations",
    "validation_event"
);
pub(super) fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/automation-flow-validations",
            get(validations).post(request_validation),
        )
        .route(
            "/api/v1/schools/{school}/automation-flow-validations/{id}/cancellations",
            post(cancel_validation),
        )
        .route(
            "/api/v5/device-execution/flow-validations",
            get(device_validations),
        )
        .route(
            "/api/v5/device-execution/flow-validations/{id}/acceptances",
            post(accept_validation),
        )
        .route(
            "/api/v5/device-execution/flow-validations/{id}/events",
            post(validation_event),
        )
        .route(
            "/api/v1/platform/automation-flows",
            get(flows).post(create_flow),
        )
        .route(
            "/api/v1/platform/automation-flows/{id}/releases",
            post(publish),
        )
        .route("/api/v1/platform/automation-flow-releases", get(releases))
        .route(
            "/api/v1/platform/automation-flow-releases/{id}/revocations",
            post(revoke),
        )
        .route(
            "/api/v1/platform/automation-pacing-profiles",
            get(pacing).post(create_pacing),
        )
        .route(
            "/api/v1/platform/automation-execution-failures",
            get(failures),
        )
        .route(
            "/api/v1/schools/{school}/automation-flow-releases",
            get(school_releases),
        )
        .route(
            "/api/v1/schools/{school}/automation-device-groups",
            get(groups).post(save_group),
        )
        .route(
            "/api/v1/schools/{school}/automation-execution-assignments",
            get(assignments).post(assign),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-settings",
            post(enable),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-previews",
            post(preview),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-previews/{id}/confirmations",
            post(confirm),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-batches",
            get(batches),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-batches/{id}",
            get(batch),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-batches/{id}/controls",
            post(control),
        )
        .route(
            "/api/v1/schools/{school}/automation-dispatch-batches/{id}/remaining-previews",
            post(rebuild),
        )
        .route(
            "/api/v1/schools/{school}/automation-execution-failures",
            get(school_failures),
        )
        .route("/api/v5/device-execution/task-inbox", get(inbox))
        .route(
            "/api/v5/device-execution/execution-authorization-context",
            get(authorization_context),
        )
        .route(
            "/api/v5/device-execution/execution-authorizations",
            post(authorization),
        )
        .route("/api/v5/device-execution/heartbeats", post(heartbeat))
        .route(
            "/api/v5/device-execution/task-deliveries/{id}/receipts",
            post(received),
        )
        .route(
            "/api/v5/device-execution/task-deliveries/{id}/preparation-leases",
            post(lease),
        )
        .route(
            "/api/v5/device-execution/task-deliveries/{id}/events",
            post(stage),
        )
        .route(
            "/api/v5/device-execution/task-deliveries/{id}/failures",
            post(failure),
        )
        .route(
            "/api/v5/device-execution/task-deliveries/{id}/diagnostics",
            post(diagnostic),
        )
        .route(
            "/api/v5/device-execution/claims/{id}/preparations",
            post(prepare),
        )
}
