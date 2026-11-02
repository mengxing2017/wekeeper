use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    distribution::{
        domain::workflows::*, north::local::workflows::Service,
        south::adapter::postgres::workflows::Repository,
    },
    http::jsonapi::{ApiResponse, Input, collection, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use uuid::Uuid;
fn service(a: &Application) -> Service {
    Service(Repository(a.pool.clone()))
}
async fn options(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "official-usage-options",
        service(&a).options(&actor, Some(s)).await?,
    ))
}
async fn platform_options(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "official-usage-options",
        service(&a).options(&actor, None).await?,
    ))
}
async fn requests(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(p): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "business-requests",
        service(&a).requests(&actor, Some(s), p.clone()).await?,
        &p,
    ))
}
async fn platform_requests(
    State(a): State<Application>,
    actor: Actor,
    Query(p): Query<Page>,
) -> Result<ApiResponse> {
    Ok(collection(
        "business-requests",
        service(&a).requests(&actor, None, p.clone()).await?,
        &p,
    ))
}
async fn decision(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "business-requests",
        service(&a)
            .command(
                &actor,
                None,
                Command::Decision {
                    id,
                    input: input.decode("business-requests")?,
                },
            )
            .await?,
    ))
}
macro_rules! school_handler {
    ($name:ident,$kind:literal,$variant:ident,$output:literal) => {
        async fn $name(
            State(a): State<Application>,
            actor: Actor,
            Path(s): Path<Uuid>,
            input: Input,
        ) -> Result<ApiResponse> {
            Ok(resource(
                $output,
                service(&a)
                    .command(&actor, Some(s), Command::$variant(input.decode($kind)?))
                    .await?,
            ))
        }
    };
}
school_handler!(
    template_preview,
    "template-previews",
    TemplatePreview,
    "template-previews"
);
school_handler!(
    template_confirm,
    "template-publications",
    TemplateConfirm,
    "templates"
);
school_handler!(
    usage_preview,
    "official-usage-previews",
    UsagePreview,
    "official-usage-previews"
);
school_handler!(
    usage_confirm,
    "official-usages",
    UsageConfirm,
    "official-usages"
);
async fn channel_request(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "business-requests",
        service(&a)
            .command(
                &actor,
                Some(s),
                Command::Request {
                    kind: "channel".into(),
                    input: input.decode("broadcast-access-requests")?,
                },
            )
            .await?,
    ))
}
async fn issue(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "business-requests",
        service(&a)
            .command(
                &actor,
                Some(s),
                Command::Request {
                    kind: "broadcast_issue".into(),
                    input: input.decode("broadcast-issues")?,
                },
            )
            .await?,
    ))
}
async fn policy_preview(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "member-policy-previews",
        service(&a)
            .command(
                &actor,
                None,
                Command::PolicyPreview(input.decode("member-policy-previews")?),
            )
            .await?,
    ))
}
async fn policy_confirm(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "member-policies",
        service(&a)
            .command(
                &actor,
                None,
                Command::PolicyConfirm(input.decode("member-policies")?),
            )
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/official-usage-options",
            get(options),
        )
        .route(
            "/api/v1/platform/official-usage-options",
            get(platform_options),
        )
        .route("/api/v1/schools/{school}/business-requests", get(requests))
        .route("/api/v1/platform/business-requests", get(platform_requests))
        .route(
            "/api/v1/platform/business-requests/{id}/decisions",
            post(decision),
        )
        .route(
            "/api/v1/schools/{school}/broadcast-access-requests",
            post(channel_request),
        )
        .route("/api/v1/schools/{school}/broadcast-issues", post(issue))
        .route(
            "/api/v1/schools/{school}/template-previews",
            post(template_preview),
        )
        .route(
            "/api/v1/schools/{school}/template-publications",
            post(template_confirm),
        )
        .route(
            "/api/v1/schools/{school}/official-usage-previews",
            post(usage_preview),
        )
        .route(
            "/api/v1/schools/{school}/official-usages",
            post(usage_confirm),
        )
        .route(
            "/api/v1/platform/member-policy-previews",
            post(policy_preview),
        )
        .route("/api/v1/platform/member-policies", post(policy_confirm))
}
