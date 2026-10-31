//! HTTP is only typed wire conversion. SQL and use-case authorization stay in the repository.
use super::*;
use crate::distribution::domain::mobile_delivery::{AutomationCommand, AutomationQuery};
fn decode(input: Input, kind: &str, action: &str) -> Result<Command> {
    let mut attributes = input.decode::<Value>(kind)?;
    let object = attributes
        .as_object_mut()
        .ok_or(Error::Invalid("attributes 必须为对象".into()))?;
    if object.contains_key("action") || object.contains_key("operation") {
        return Err(Error::Invalid("不接受 action 或 operation 字段".into()));
    }
    object.insert("action".into(), json!(action));
    let input = serde_json::from_value::<AutomationCommand>(attributes)
        .map_err(|_| Error::Invalid("字段无效或包含未知配置".into()))?;
    Ok(Command::Automation {
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
                    Read::Automation(AutomationQuery::$query),
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
                    Read::Automation(AutomationQuery::$query),
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
                    Read::Automation(AutomationQuery::$query),
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
school_read!(apps, "automation-apps", Apps);
school_read!(bindings, "device-app-bindings", Bindings);
school_read!(validations, "automation-validations", Validations);
school_read!(runs, "automation-runs", Runs);
school_item_read!(applications, "automation-applications", Applications);
school_item_read!(validation, "automation-validations", Validation);
school_item_read!(run, "automation-runs", Run);
school_item_read!(school_resolution, "app-binding-resolutions", Resolution);
school_write!(bind, "device-app-bindings", "bind_application");
school_write!(validate, "automation-validations", "request_validation");
school_write!(preview, "automation-business-previews", "preview_business");
school_item_write!(cancel, "automation-validations", "cancel_validation");
school_item_write!(
    confirm,
    "automation-business-confirmations",
    "confirm_business"
);
platform_read!(versions, "automation-components", Components);
platform_read!(releases, "automation-releases", Releases);
platform_item_write!(publish, "automation-releases", "publish_component");
platform_item_write!(revoke, "automation-releases", "revoke_release");
async fn create(
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
            decode(input, "automation-components", "create_component")?,
        )
        .await?;
    Ok(response("automation-components", v, None, true))
}
device_write!(environment, "device-environments", "report_environment");
device_write!(readiness, "execution-readiness", "prepare_execution");
device_item_write!(accept, "device-validations", "accept_validation");
device_item_write!(event, "device-validations", "report_validation");
device_item_write!(
    preparation_event,
    "execution-readiness",
    "report_preparation"
);
device_item_write!(ready, "execution-readiness", "application_ready");
async fn jobs(
    State(a): State<Application>,
    DevicePrincipal(p): DevicePrincipal,
) -> Result<Response> {
    let page = Page::default();
    let v = a
        .mobile_delivery()
        .query(
            p,
            Scope {
                school: None,
                id: None,
            },
            Read::Automation(AutomationQuery::ValidationJobs),
            page.clone(),
        )
        .await?;
    Ok(response("device-validations", v, Some(&page), false))
}
async fn resolution(
    State(a): State<Application>,
    DevicePrincipal(p): DevicePrincipal,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let v = a
        .mobile_delivery()
        .query(
            p,
            Scope {
                school: None,
                id: Some(id),
            },
            Read::Automation(AutomationQuery::Resolution),
            Page::default(),
        )
        .await?;
    Ok(response("app-binding-resolutions", v, None, false))
}
pub(super) fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/schools/{school}/automation-apps", get(apps))
        .route(
            "/api/v1/schools/{school}/automation-devices/{id}/applications",
            get(applications),
        )
        .route(
            "/api/v1/schools/{school}/device-app-bindings",
            get(bindings).post(bind),
        )
        .route(
            "/api/v1/schools/{school}/device-app-bindings/{id}/capability-resolution",
            get(school_resolution),
        )
        .route(
            "/api/v1/schools/{school}/automation-validations",
            get(validations).post(validate),
        )
        .route(
            "/api/v1/schools/{school}/automation-validations/{id}",
            get(validation),
        )
        .route(
            "/api/v1/schools/{school}/automation-validations/{id}/cancellations",
            post(cancel),
        )
        .route(
            "/api/v1/schools/{school}/automation-business-previews",
            post(preview),
        )
        .route(
            "/api/v1/schools/{school}/automation-business-previews/{id}/confirmations",
            post(confirm),
        )
        .route("/api/v1/schools/{school}/automation-runs", get(runs))
        .route("/api/v1/schools/{school}/automation-runs/{id}", get(run))
        .route(
            "/api/v1/platform/automation-adaptations/components",
            get(versions).post(create),
        )
        .route(
            "/api/v1/platform/automation-adaptations/components/{id}/releases",
            post(publish),
        )
        .route(
            "/api/v1/platform/automation-adaptations/releases",
            get(releases),
        )
        .route(
            "/api/v1/platform/automation-adaptations/releases/{id}/revocations",
            post(revoke),
        )
        .route(
            "/api/v4/device-execution/environment-snapshots",
            post(environment),
        )
        .route(
            "/api/v4/device-execution/app-binding-resolutions/{id}",
            get(resolution),
        )
        .route("/api/v4/device-execution/validation-jobs", get(jobs))
        .route(
            "/api/v4/device-execution/validation-jobs/{id}/acceptances",
            post(accept),
        )
        .route(
            "/api/v4/device-execution/validation-jobs/{id}/events",
            post(event),
        )
        .route(
            "/api/v4/device-execution/execution-readiness",
            post(readiness),
        )
        .route(
            "/api/v4/device-execution/execution-readiness/{id}/events",
            post(preparation_event),
        )
        .route(
            "/api/v4/device-execution/execution-readiness/{id}/completions",
            post(ready),
        )
}
