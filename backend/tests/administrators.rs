use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use wekeeper_api::{
    business::{Actor, Error},
    identity::{
        north::local::{AdministratorCreate, AdministratorEdit, Service},
        south::adapter::postgres::Repository,
    },
};
fn actor(admin: bool) -> Actor {
    serde_json::from_value(json!({"id":"00000000-0000-4000-8000-000000000001","username":"root","display_name":"管理员","platform_admin":admin,"bound_school_id":null,"grants":[],"wecom_authenticated":false})).unwrap()
}
fn service() -> Service {
    Service(Repository(
        PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap(),
    ))
}
#[tokio::test]
async fn administrator_crud_rejects_ordinary_accounts_before_database_access() {
    let service = service();
    let actor = actor(false);
    assert!(matches!(
        service.list(&actor, "").await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service
            .create_administrator(
                &actor,
                AdministratorCreate {
                    username: "admin".into(),
                    display_name: "管理员".into(),
                    password: "new-password-123".into()
                }
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service
            .edit_administrator(
                &actor,
                Uuid::now_v7(),
                AdministratorEdit {
                    display_name: "改名".into(),
                    active: true
                }
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.remove_administrator(&actor, Uuid::now_v7()).await,
        Err(Error::Forbidden)
    ));
}
#[tokio::test]
async fn administrators_cannot_disable_or_delete_their_own_login() {
    let service = service();
    let actor = actor(true);
    assert!(matches!(
        service
            .edit_administrator(
                &actor,
                actor.id,
                AdministratorEdit {
                    display_name: "改名".into(),
                    active: false
                }
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        service.remove_administrator(&actor, actor.id).await,
        Err(Error::Invalid(_))
    ));
}
#[tokio::test]
async fn administrator_creation_validates_credentials_and_edit_rejects_empty_names() {
    let service = service();
    let actor = actor(true);
    for (username, name, password) in [
        (" ", "姓名", "new-password-123"),
        ("admin", " ", "new-password-123"),
        ("admin", "姓名", "short"),
    ] {
        assert!(matches!(
            service
                .create_administrator(
                    &actor,
                    AdministratorCreate {
                        username: username.into(),
                        display_name: name.into(),
                        password: password.into()
                    }
                )
                .await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(matches!(
        service
            .edit_administrator(
                &actor,
                actor.id,
                AdministratorEdit {
                    display_name: " ".into(),
                    active: true
                }
            )
            .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn administrator_routes_require_login_and_use_strict_inputs() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    for (method, path) in [
        ("GET", "/api/v1/platform/administrators?search=root"),
        ("POST", "/api/v1/platform/administrators"),
        (
            "PUT",
            "/api/v1/platform/administrators/00000000-0000-4000-8000-000000000002",
        ),
        (
            "DELETE",
            "/api/v1/platform/administrators/00000000-0000-4000-8000-000000000002",
        ),
    ] {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let app = wekeeper_api::http::app_router(wekeeper_api::bootstrap::build_application(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
    assert!(serde_json::from_value::<AdministratorCreate>(json!({"username":"admin","display_name":"姓名","password":"password-1234","platform_admin":false})).is_err());
    assert!(
        serde_json::from_value::<AdministratorEdit>(
            json!({"username":"renamed","display_name":"姓名","active":true})
        )
        .is_err()
    );
}

#[tokio::test]
async fn administrator_wire_shape_matches_shared_contract() {
    use axum::{body::to_bytes, response::IntoResponse};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/administrator.json"
    ))
    .unwrap();
    let row = json!({"id":"00000000-0000-4000-8000-000000000002","username":"admin","display_name":"维护员","active":true,"platform_admin":true});
    let response = wekeeper_api::http::jsonapi::resource("administrators", row).into_response();
    let wire: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(wire, fixture);
}
