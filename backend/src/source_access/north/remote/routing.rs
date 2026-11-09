use super::*;
use crate::source_access::{
    domain::routing::*, north::local::routing::Service as RoutingService,
    south::adapter::postgres::routing::RoutingRepository,
};
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceFilter {
    channel: Option<Channel>,
    workspace_id: Option<uuid::Uuid>,
    page: Option<i64>,
}
async fn run(
    a: Application,
    actor: Actor,
    command: Command,
    kind: &str,
) -> std::result::Result<Response, Response> {
    let route = match &command {
        Command::Configure { route, .. }
        | Command::Lifecycle { route, .. }
        | Command::Manager { route, .. } => Some(route.clone()),
        _ => None,
    };
    let service = RoutingService(RoutingRepository(a.pool));
    let value = match service.execute(&actor, command).await {
        Ok(value) => value,
        Err(Fault("revision_conflict")) if route.is_some() => {
            let current = service
                .execute(
                    &actor,
                    Command::Configuration {
                        route: route.unwrap(),
                    },
                )
                .await
                .map_err(|e| e.response(&actor.request_id))?;
            return Err((StatusCode::CONFLICT,[("content-type",MEDIA_TYPE)],Json(json!({"errors":[{"status":"409","code":"revision_conflict","title":"配置已更新，请核对最新版本","meta":{"request_id":actor.request_id,"current":current,"retryable":false}}]}))).into_response());
        }
        Err(error) => return Err(error.response(&actor.request_id)),
    };
    Ok(if value.get("id").is_some() {
        crate::http::jsonapi::resource(kind, value).into_response()
    } else {
        crate::http::jsonapi::singleton(kind, "result", value).into_response()
    })
}
async fn entities(
    State(a): State<Application>,
    actor: Actor,
    Path(kind): Path<String>,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Entities { kind: kind.clone() }, &kind).await
}
async fn entity_create(
    State(a): State<Application>,
    actor: Actor,
    Path(kind): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<EntityInput>(&kind)
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Entity {
            kind: kind.clone(),
            id: None,
            input: v,
        },
        &kind,
    )
    .await
}
async fn entity_update(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id)): Path<(String, uuid::Uuid)>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<EntityInput>(&kind)
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Entity {
            kind: kind.clone(),
            id: Some(id),
            input: v,
        },
        &kind,
    )
    .await
}
async fn configuration(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Configuration { route },
        "source-route-configurations",
    )
    .await
}
async fn configure(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<ConfigurationInput>("source-route-configurations")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Configure { route, input: v },
        "source-route-configurations",
    )
    .await
}
async fn check(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Check { route }, "source-route-checks").await
}
async fn lifecycle(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<LifecycleInput>("source-route-lifecycle")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Lifecycle { route, input: v },
        "source-route-lifecycle",
    )
    .await
}
async fn revisions(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    Query(q): Query<ResourceFilter>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Revisions {
            route,
            page: q.page.unwrap_or(1),
        },
        "source-route-revisions",
    )
    .await
}
async fn managers(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Managers { route },
        "source-route-managers",
    )
    .await
}
async fn manager(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<ManagerInput>("source-route-managers")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Manager { route, input: v },
        "source-route-managers",
    )
    .await
}
async fn resources(
    State(a): State<Application>,
    actor: Actor,
    Query(q): Query<ResourceFilter>,
) -> std::result::Result<Response, Response> {
    let channel = q
        .channel
        .ok_or_else(|| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::Resources {
            channel,
            workspace: q.workspace_id,
        },
        "source-route-resources",
    )
    .await
}
async fn group_targets(
    State(a): State<Application>,
    actor: Actor,
    Path(group): Path<uuid::Uuid>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::GroupTargets { group },
        "routing-group-targets",
    )
    .await
}
async fn set_group_targets(
    State(a): State<Application>,
    actor: Actor,
    Path(group): Path<uuid::Uuid>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<GroupTargetInput>("routing-group-targets")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    run(
        a,
        actor,
        Command::SetGroupTargets { group, input: v },
        "routing-group-targets",
    )
    .await
}
async fn handoff(
    State(a): State<Application>,
    actor: Actor,
    Path(route): Path<String>,
) -> std::result::Result<Response, Response> {
    run(
        a,
        actor,
        Command::Handoff { route },
        "source-route-handoffs",
    )
    .await
}
async fn assigned(
    State(a): State<Application>,
    actor: Actor,
) -> std::result::Result<Response, Response> {
    run(a, actor, Command::Assigned, "source-route-assignments").await
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/source-route-assignments", get(assigned))
        .route(
            "/api/v2/admin/routing/{kind}",
            get(entities).post(entity_create),
        )
        .route(
            "/api/v2/admin/routing/{kind}/{id}",
            axum::routing::patch(entity_update),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/configuration",
            get(configuration).put(configure),
        )
        .route("/api/v2/admin/source-routes/{route}/checks", get(check))
        .route(
            "/api/v2/admin/source-routes/{route}/lifecycle",
            post(lifecycle),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/revisions",
            get(revisions),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/managers",
            get(managers).put(manager),
        )
        .route("/api/v2/admin/source-routes/{route}/handoff", post(handoff))
        .route("/api/v2/admin/source-route-resources", get(resources))
        .route(
            "/api/v2/admin/routing-groups/{group}/targets",
            get(group_targets).put(set_group_targets),
        )
}
