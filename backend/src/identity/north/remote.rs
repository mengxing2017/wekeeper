use super::local::{AdministratorCreate, AdministratorEdit, Login};
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Page, Result},
    http::jsonapi::{ApiResponse, Input, completed, resource, singleton},
};
use axum::{
    Router,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde_json::{Value, json};
use uuid::Uuid;
impl FromRequestParts<Application> for Actor {
    type Rejection = Error;
    async fn from_request_parts(parts: &mut Parts, state: &Application) -> Result<Self> {
        let cookie = parts
            .headers
            .get(header::COOKIE)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| {
                s.split(';')
                    .find_map(|v| v.trim().strip_prefix("wekeeper_session="))
            });
        let token = cookie.ok_or(Error::Unauthorized)?;
        let mut actor = state.identity.authenticate(token, "browser").await?;
        if actor.wecom_authenticated {
            state.integrations.directory().ensure_config().await?;
        }
        actor.request_id = parts
            .headers
            .get("x-request-id")
            .and_then(|x| x.to_str().ok())
            .unwrap_or("")
            .chars()
            .take(64)
            .collect();
        if actor.browser && !matches!(parts.method.as_str(), "GET" | "HEAD" | "OPTIONS") {
            check_origin(&parts.headers, state)?;
            if parts
                .headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                != Some(actor.csrf.as_str())
            {
                return Err(Error::Forbidden);
            }
        }
        Ok(actor)
    }
}
pub fn check_origin(h: &HeaderMap, a: &Application) -> Result<()> {
    let origin = h.get(header::ORIGIN).and_then(|s| s.to_str().ok());
    if origin.is_some_and(|origin| origin_allowed(origin, &a.options)) {
        Ok(())
    } else {
        tracing::warn!("request Origin does not match WEKEEPER_PUBLIC_ORIGIN");
        Err(Error::Forbidden)
    }
}

fn origin_allowed(origin: &str, options: &crate::bootstrap::Options) -> bool {
    if origin == options.public_origin {
        return true;
    }
    if !options.insecure_local_auth {
        return false;
    }
    // Only explicit loopback aliases are interchangeable, with the same scheme
    // and port. Never trust arbitrary Host/Forwarded headers or other ports.
    for scheme in ["http", "https"] {
        for host in ["localhost", "127.0.0.1", "[::1]"] {
            let prefix = format!("{scheme}://{host}");
            if let Some(suffix) = options.public_origin.strip_prefix(&prefix)
                && (suffix.is_empty()
                    || suffix
                        .strip_prefix(':')
                        .is_some_and(|port| port.parse::<u16>().is_ok()))
            {
                return ["localhost", "127.0.0.1", "[::1]"]
                    .iter()
                    .any(|alias| origin == format!("{scheme}://{alias}{suffix}"));
            }
        }
    }
    false
}

pub fn cookie(token: &str, secure: bool, max_age: u32) -> String {
    format!(
        "wekeeper_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}
async fn login(
    State(a): State<Application>,
    headers: HeaderMap,
    input_document: Input,
) -> Result<Response> {
    let input = input_document.decode::<Login>("sessions")?;
    check_origin(&headers, &a)?;
    let (t, user) = a.identity.login(input).await?;
    let c = cookie(&t, !a.options.insecure_local_auth, 28800);
    Ok((
        [
            (header::SET_COOKIE, c),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        singleton("sessions", "current", json!({"user":user,"csrf":user.csrf})),
    )
        .into_response())
}
async fn session(State(_a): State<Application>, actor: Actor) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        singleton(
            "sessions",
            "current",
            json!({"user":actor,"csrf":actor.csrf}),
        ),
    )
}
async fn logout(State(a): State<Application>, actor: Actor) -> Result<Response> {
    a.identity.logout(&actor).await?;
    Ok((
        [(
            header::SET_COOKIE,
            cookie("", !a.options.insecure_local_auth, 0),
        )],
        completed(),
    )
        .into_response())
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Query(query): Query<Page>,
) -> Result<ApiResponse> {
    Ok(resource(
        "administrators",
        a.identity
            .list(&actor, query.search.as_deref().unwrap_or(""))
            .await?,
    ))
}
async fn create_administrator(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    let value = a
        .identity
        .create_administrator(
            &actor,
            input.decode::<AdministratorCreate>("administrators")?,
        )
        .await?;
    Ok(resource("administrators", value))
}
async fn edit_administrator(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<ApiResponse> {
    let value = a
        .identity
        .edit_administrator(
            &actor,
            id,
            input.decode::<AdministratorEdit>("administrators")?,
        )
        .await?;
    Ok(resource("administrators", value))
}
async fn remove_administrator(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse> {
    a.identity.remove_administrator(&actor, id).await?;
    Ok(completed())
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordReset {
    password: String,
}
async fn reset(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<PasswordReset>("administrators")?;
    a.identity.reset(&actor, id, &v.password).await?;
    Ok(completed())
}
async fn profile(
    State(a): State<Application>,
    actor: Actor,
    input_document: Input,
) -> Result<ApiResponse> {
    let v = input_document.decode::<Value>("accounts")?;
    Ok(singleton(
        "accounts",
        actor.id,
        a.identity
            .profile(&actor, v["display_name"].as_str().unwrap_or(""))
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/auth/sessions", post(login))
        .route("/api/v1/auth/session", get(session).delete(logout))
        .route(
            "/api/v1/platform/administrators",
            get(list).post(create_administrator),
        )
        .route(
            "/api/v1/platform/administrators/{id}",
            put(edit_administrator).delete(remove_administrator),
        )
        .route("/api/v1/platform/administrators/{id}/password", post(reset))
        .route("/api/v1/profile", put(profile))
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    #[test]
    fn loopback_aliases_only_in_local_mode_with_matching_scheme_and_port() {
        let mut options = crate::bootstrap::Options {
            public_origin: "http://localhost:5173".into(),
            insecure_local_auth: true,
        };
        for origin in [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://[::1]:5173",
        ] {
            assert!(origin_allowed(origin, &options));
        }
        for origin in [
            "null",
            "http://localhost:5174",
            "https://localhost:5173",
            "http://localhost.evil.test:5173",
            "http://evil.test:5173",
            "http://127.0.0.1:5173/path",
        ] {
            assert!(!origin_allowed(origin, &options));
        }
        options.insecure_local_auth = false;
        assert!(!origin_allowed("http://127.0.0.1:5173", &options));
        assert!(origin_allowed("http://localhost:5173", &options));
        options.insecure_local_auth = true;
        options.public_origin = "https://example.com".into();
        assert!(!origin_allowed("http://localhost:5173", &options));
    }
}
