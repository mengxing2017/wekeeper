use crate::{
    bootstrap::Application,
    business::Actor,
    source_access::{domain::*, north::local::Service, south::adapter::postgres::Repository},
};
use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
fn header<'a>(h: &'a HeaderMap, name: &str) -> Result<&'a str> {
    ensure(h.get_all(name).iter().count() == 1, "invalid_request")?;
    h.get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or(Fault("invalid_request"))
}
impl Fault {
    fn response(self, request_id: &str) -> Response {
        let status = match self.0 {
            "authentication_failed" | "credential_unavailable" | "timestamp_expired" => 401,
            "forbidden" | "source_disabled" | "route_unavailable" => 403,
            "not_found" | "receipt_not_found" => 404,
            "replay_detected" | "event_conflict" | "revision_conflict" | "cursor_stale" => 409,
            "temporarily_unavailable" | "source_key_configuration_required" => 503,
            "unsupported_media_type" => 415,
            "payload_too_large" => 413,
            "rate_limited" => 429,
            _ => 400,
        };
        let configuration_required = self.0 == "source_key_configuration_required";
        let code = if configuration_required {
            "temporarily_unavailable"
        } else {
            self.0
        };
        let title = if configuration_required {
            "调用方凭据加密服务未配置，请联系部署人员完成配置"
        } else {
            "请求未完成"
        };
        let retryable = !configuration_required
            && (matches!(status, 429 | 500 | 503)
                || matches!(self.0, "replay_detected" | "timestamp_expired"));
        let mut response=(StatusCode::from_u16(status).unwrap(),[("content-type",MEDIA_TYPE)],Json(json!({"errors":[{"status":status.to_string(),"code":code,"title":title,"meta":{"retryable":retryable,"supported_versions":[2],"request_id":request_id}}]}))).into_response();
        if status == 429 {
            response
                .headers_mut()
                .insert("retry-after", axum::http::HeaderValue::from_static("60"));
        }
        response
    }
}
impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        self.response("")
    }
}
// Coarse process budget before database lookup; durable per-key limits follow authentication.
fn pre_auth_budget() -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static BUCKET: AtomicU64 = AtomicU64::new(0);
    let seconds = chrono::Utc::now().timestamp().max(0) as u64;
    BUCKET
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            let previous = old >> 20;
            let count = old & ((1 << 20) - 1);
            if previous != seconds {
                Some((seconds << 20) | 1)
            } else if count < 1000 {
                Some(old + 1)
            } else {
                None
            }
        })
        .map_err(|_| Fault("rate_limited"))?;
    Ok(())
}
async fn machine(State(a): State<Application>, req: Request) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let result = async {
        pre_auth_budget()?;
        let path = req.uri().path().to_string();
        let method = req.method().as_str().to_string();
        let h = req.headers();
        ensure(
            req.uri().query().is_none()
                && !path.contains('%')
                && !path.contains("//")
                && !path.ends_with('/'),
            "invalid_request",
        )?;
        ensure(header(h, "x-source-version")? == "2", "unsupported_version")?;
        ensure(
            header(h, "accept")? == MEDIA_TYPE && !h.contains_key("content-encoding"),
            "unsupported_media_type",
        )?;
        if method == "POST" {
            ensure(
                header(h, "content-type")? == MEDIA_TYPE,
                "unsupported_media_type",
            )?;
        }
        let nonce = header(h, "x-source-nonce")?;
        let identity = header(h, "idempotency-key")?;
        let n: uuid::Uuid = nonce.parse().map_err(|_| Fault("invalid_request"))?;
        let i: uuid::Uuid = identity.parse().map_err(|_| Fault("invalid_request"))?;
        ensure(
            n.to_string() == nonce && i.to_string() == identity,
            "invalid_request",
        )?;
        let key = header(h, "x-source-key-id")?.to_string();
        ensure(identifier(&key), "invalid_request")?;
        let signed = Signed {
            key_id: key,
            timestamp: header(h, "x-source-timestamp")?.into(),
            nonce: n,
            identity: i,
            signature: header(h, "x-source-signature")?.into(),
            method,
            path,
            request_id: request_id.clone(),
        };
        let body = axum::body::to_bytes(req.into_body(), MAX_BODY)
            .await
            .map_err(|_| Fault("payload_too_large"))?;
        let (fresh, value) = Service(a.pool).machine(signed, body.to_vec()).await?;
        Ok::<_, Fault>(
            (
                if fresh {
                    StatusCode::ACCEPTED
                } else {
                    StatusCode::OK
                },
                [("content-type", MEDIA_TYPE)],
                Json(value),
            )
                .into_response(),
        )
    }
    .await;
    match result {
        Ok(r) => r,
        Err(e) => {
            let code = e.0;
            tracing::info!(request_id=%request_id,code,"source request rejected");
            let mut r = e.response(&request_id);
            if let Ok(v) = request_id.parse() {
                r.headers_mut().insert("x-request-id", v);
            }
            r
        }
    }
}
type Filter = ManagementFilter;
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Path(kind): Path<String>,
    Query(q): Query<Filter>,
) -> std::result::Result<Response, Response> {
    let v = Service(a.pool)
        .list(&actor, &kind, q)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(&kind, "list", v).into_response())
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id)): Path<(String, String)>,
) -> std::result::Result<Response, Response> {
    let v = Service(a.pool)
        .manage(&actor, &kind, Some(&id), "GET", None, 1, "", "", None)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(&kind, &id, v).into_response())
}
async fn create(
    State(a): State<Application>,
    actor: Actor,
    Path(kind): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<AdminInput>(&kind)
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    let v = Service(a.pool)
        .manage(&actor, &kind, None, "POST", Some(v), 1, "", "", None)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::resource(&kind, v).into_response())
}
async fn update(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id)): Path<(String, String)>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<AdminInput>(&kind)
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    let v = Service(a.pool)
        .manage(&actor, &kind, Some(&id), "PATCH", Some(v), 1, "", "", None)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::resource(&kind, v).into_response())
}
async fn delete(
    State(a): State<Application>,
    actor: Actor,
    Path((kind, id)): Path<(String, String)>,
) -> std::result::Result<Response, Response> {
    let v: AdminInput = serde_json::from_value(json!({})).unwrap();
    let v = Service(a.pool)
        .manage(&actor, &kind, Some(&id), "DELETE", Some(v), 1, "", "", None)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::resource(&kind, v).into_response())
}
async fn associations(
    State(a): State<Application>,
    actor: Actor,
    Path(key): Path<String>,
    Query(q): Query<Filter>,
) -> std::result::Result<Response, Response> {
    let v = Repository(a.pool)
        .associations(&actor, &key, q.page.unwrap_or(1))
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton("source-key-routes", key, v).into_response())
}
fn secret_response(value: CredentialSecret) -> Response {
    let mut response =
        crate::http::jsonapi::resource("source-credential-secret-reveals", value).into_response();
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
async fn reveal_secret(
    State(a): State<Application>,
    actor: Actor,
    Path(key): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    input
        .decode::<SecretRevealInput>("source-credential-secret-reveals")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    let value = Repository(a.pool)
        .reveal_secret(&actor, &key)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(secret_response(value))
}
async fn association(
    State(a): State<Application>,
    actor: Actor,
    Path((key, route)): Path<(String, String)>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let v = input
        .decode::<AssociationInput>("source-key-routes")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    let value = Repository(a.pool)
        .set_association(&actor, &key, &route, &v.status, v.revision)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::resource("source-key-routes", value).into_response())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v2/admin/source-credentials/{key}/secret-reveals",
            post(reveal_secret),
        )
        .route(
            "/api/v2/admin/source-credentials/{key}/routes/{route}",
            axum::routing::put(association),
        )
        .route("/api/v2/source-authentications", post(machine))
        .route("/api/v2/source-route-checks", post(machine))
        .route("/api/v2/source-events", post(machine))
        .route("/api/v2/source-receipts/{id}", get(machine))
        .route("/api/v2/admin/{kind}", get(list).post(create))
        .route(
            "/api/v2/admin/{kind}/{id}",
            get(detail).patch(update).delete(delete),
        )
        .route(
            "/api/v2/admin/source-credentials/{key}/routes",
            get(associations),
        )
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request as HttpRequest,
    };
    use tower::ServiceExt;
    #[tokio::test]
    async fn secret_read_requires_a_session_and_an_explicit_post() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1:1/test")
            .unwrap();
        let app = router().with_state(crate::bootstrap::build_application(pool));
        for (method, status) in [
            ("POST", StatusCode::UNAUTHORIZED),
            ("GET", StatusCode::METHOD_NOT_ALLOWED),
        ] {
            let response = app
                .clone()
                .oneshot(
                    HttpRequest::builder()
                        .method(method)
                        .uri("/api/v2/admin/source-credentials/test-key/secret-reveals")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
    }
    #[tokio::test]
    async fn secret_read_checks_management_permission_before_database_access() {
        let actor: Actor = serde_json::from_value(json!({
            "id":uuid::Uuid::nil(),"username":"operator","display_name":"operator",
            "platform_admin":false,"bound_school_id":null,"grants":[],"wecom_authenticated":false
        }))
        .unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1:1/test")
            .unwrap();
        let result = Repository(pool).reveal_secret(&actor, "test-key").await;
        assert_eq!(result.err().unwrap().0, "forbidden");
    }
    #[tokio::test]
    async fn rejects_invalid_machine_protocol_before_database_access() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1:1/test")
            .unwrap();
        let app = router().with_state(crate::bootstrap::build_application(pool));
        for (headers, code) in [
            (vec![("x-source-version", "1")], "unsupported_version"),
            (
                vec![("x-source-version", "2"), ("accept", "text/html")],
                "unsupported_media_type",
            ),
        ] {
            let mut req = HttpRequest::builder()
                .method("POST")
                .uri("/api/v2/source-events");
            for (k, v) in headers {
                req = req.header(k, v);
            }
            let r = app
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert!(r.status().is_client_error());
            let value: serde_json::Value =
                serde_json::from_slice(&to_bytes(r.into_body(), MAX_BODY).await.unwrap()).unwrap();
            assert_eq!(value["errors"][0]["code"], code);
        }
    }
}
#[cfg(test)]
mod response_tests {
    use super::*;
    #[tokio::test]
    async fn secret_response_matches_contract_and_disables_caching() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-credential-secret.json"
        ))
        .unwrap();
        let response = secret_response(CredentialSecret {
            id: "fixture-key".into(),
            key_id: "fixture-key".into(),
            secret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8".into(),
        });
        assert_eq!(response.headers()["cache-control"], "no-store");
        let actual: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), MAX_BODY)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    #[tokio::test]
    async fn configuration_failure_is_actionable_without_exposing_secrets() {
        let response = Fault("source_key_configuration_required").response("request-config");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let value: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), MAX_BODY)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["errors"][0]["code"], "temporarily_unavailable");
        assert_eq!(value["errors"][0]["meta"]["retryable"], false);
        assert_eq!(value["errors"][0]["meta"]["request_id"], "request-config");
        assert!(
            value["errors"][0]["title"]
                .as_str()
                .unwrap()
                .contains("加密服务未配置")
        );
    }
    #[tokio::test]
    async fn published_error_matches_rust_response() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-errors-v2.json"
        ))
        .unwrap();
        let response = Fault("credential_unavailable").response("fixture-request");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let value: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), MAX_BODY)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value, expected);
    }
}
