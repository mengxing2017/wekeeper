use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;
use wekeeper_api::{bootstrap, business::Actor, database, http};
fn write_document(path: &str, mut body: Value) -> Value {
    let segments: Vec<_> = path
        .split('?')
        .next()
        .unwrap()
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let kind = match segments[2] {
        "auth" => "sessions",
        "accounts" | "profile" => "accounts",
        "platform" => segments[3],
        "wecom-distribution" => segments.get(5).copied().unwrap_or("wecom-workspaces"),
        "schools" => match segments.get(4).copied().unwrap_or("schools") {
            "members" => "accounts",
            "posts" if segments.get(6) == Some(&"comments") => "comments",
            kind => kind,
        },
        _ => panic!("unmapped write route: {path}"),
    };
    let id = body.as_object_mut().unwrap().remove("id");
    let mut resource = json!({"type":kind,"attributes":body});
    if let Some(id) = id {
        resource["id"] = id;
    }
    json!({"data":resource})
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    token: &str,
    csrf: &str,
    body: Value,
) -> (u16, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", "http://localhost:5173")
        .header("content-type", "application/vnd.api+json");
    if !token.is_empty() {
        req = req.header("cookie", format!("wekeeper_session={token}"));
    }
    if !csrf.is_empty() {
        req = req.header("x-csrf-token", csrf);
    }
    let response = app
        .clone()
        .oneshot(
            req.body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(write_document(path, body).to_string())
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let v = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, v)
}
async fn ok(app: &Router, method: &str, path: &str, token: &str, a: &Actor, v: Value) -> Value {
    let (status, v) = call(app, method, path, token, &a.csrf, v).await;
    assert_eq!(status, 200, "{method} {path}: {v}");
    let decode = |resource: &Value| {
        assert!(
            resource["type"].is_string(),
            "missing resource type: {resource}"
        );
        assert!(
            resource["id"].is_string(),
            "missing resource id: {resource}"
        );
        let mut attributes = resource["attributes"].as_object().unwrap().clone();
        assert!(!attributes.contains_key("id"));
        attributes.insert("id".into(), resource["id"].clone());
        Value::Object(attributes)
    };
    if let Some(items) = v["data"].as_array() {
        let items: Vec<_> = items.iter().map(decode).collect();
        json!(items)
    } else if v["data"].is_null() {
        Value::Null
    } else {
        decode(&v["data"])
    }
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn complete_business_flow_and_security_boundaries() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated test DB required"),
    )
    .await
    .unwrap();
    database::migrate(&pool).await.unwrap();
    let mut a = bootstrap::build_application(pool.clone());
    a.integrations.corp = "corp-test".into();
    let return_path = format!("/wecom?school={}", Uuid::new_v4());
    let oauth_state = a
        .integrations
        .repository
        .oauth_state(&return_path)
        .await
        .unwrap();
    assert_eq!(
        a.integrations
            .repository
            .consume(&oauth_state)
            .await
            .unwrap(),
        return_path
    );
    assert!(
        a.integrations
            .repository
            .consume(&oauth_state)
            .await
            .is_err()
    );
    let expired = a
        .integrations
        .repository
        .oauth_state("/wecom")
        .await
        .unwrap();
    sqlx::query("UPDATE oauth_states SET expires_at=now()-interval '1 second' WHERE state_hash=$1")
        .bind(wekeeper_api::business::hash(&expired))
        .execute(&pool)
        .await
        .unwrap();
    assert!(a.integrations.repository.consume(&expired).await.is_err());
    let suffix = Uuid::new_v4();
    let adminid = a
        .identity
        .bootstrap(
            &format!("admin{suffix}"),
            "Admin",
            "test-password-123",
            true,
        )
        .await
        .unwrap();
    let (admin_token, admin) = a
        .identity
        .0
        .session(adminid, "browser", false)
        .await
        .unwrap();
    // Repository-level delivery tests need an independently verified enterprise identity.
    let enterprise_member = Uuid::new_v4();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,$2,'Admin',1,'verified',now())")
        .bind(enterprise_member).bind(format!("admin-{suffix}")).execute(&pool).await.unwrap();
    sqlx::query("UPDATE accounts SET wecom_member_id=$2 WHERE id=$1")
        .bind(adminid)
        .bind(enterprise_member)
        .execute(&pool)
        .await
        .unwrap();
    let app = http::app_router(a.clone());
    assert_eq!(
        call(&app, "GET", "/api/v1/schools", "", "", Value::Null)
            .await
            .0,
        404
    );
    let s = Uuid::new_v4();
    let b = Uuid::new_v4();
    for (id, name) in [(s, "Workspace A"), (b, "Workspace B")] {
        ok(
            &app,
            "PUT",
            &format!("/api/v1/wecom-distribution/workspaces/{id}"),
            &admin_token,
            &admin,
            json!({"name":name}),
        )
        .await;
    }
    let memberid = a
        .identity
        .bootstrap(
            &format!("member{suffix}"),
            "Member",
            "test-password-123",
            false,
        )
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO wecom_workspace_members(school_id,account_id,role) VALUES($1,$2,'member')",
    )
    .bind(s)
    .bind(memberid)
    .execute(&pool)
    .await
    .unwrap();
    let (token, member) = a
        .identity
        .0
        .session(memberid, "browser", false)
        .await
        .unwrap();
    let root = format!("/api/v1/wecom-distribution/workspaces/{s}");
    // Historical projection is a sending-owned read model; no forum API creates content.
    let p = Uuid::new_v4();
    sqlx::query("INSERT INTO distribution_source_organizations(namespace,external_organization_id,school_id) VALUES('builtin.forum',$1,$2)").bind(s.to_string()).bind(s).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO distribution_materials(namespace,external_organization_id,external_content_id,school_id,version,title,content,allowed) VALUES('builtin.forum',$1,$2,$3,'1','Campus','Hello',true)")
        .bind(s.to_string()).bind(p.to_string()).bind(s).execute(&pool).await.unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/v1/wecom-distribution/workspaces/{b}/distribution-materials"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/distribution-materials"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    a.wecom_workspaces()
        .grant(&admin, s, &format!("member{suffix}"), "operator")
        .await
        .unwrap();
    let selected = ok(
        &app,
        "GET",
        &format!("{root}/distribution-materials?page[size]=1"),
        &token,
        &member,
        Value::Null,
    )
    .await;
    assert_eq!(selected[0]["id"], json!(p));
    assert_eq!(selected[0]["version"], "1");
    sqlx::query("UPDATE distribution_materials SET allowed=false WHERE external_content_id=$1 AND school_id=$2").bind(p.to_string()).bind(s).execute(&pool).await.unwrap();
    let selected = ok(
        &app,
        "GET",
        &format!("{root}/distribution-materials"),
        &token,
        &member,
        Value::Null,
    )
    .await;
    assert_eq!(selected.as_array().unwrap().len(), 0);
    // Wrong enterprise is rejected before any upstream request.
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/groups"),
            &admin_token,
            &admin.csrf,
            json!({"name":"Wrong","corp_id":"other-corp","chat_id":"chat","active":false})
        )
        .await
        .0,
        422
    );
    // Offline fixture: new manual groups require a real official detail response, even if inactive.
    let inactive = a.distribution_groups().0.group(&admin,s,json!({"name":"Inactive","corp_id":"corp-test","chat_id":format!("inactive-{suffix}"),"active":false})).await.unwrap();
    assert_eq!(call(&app, "POST", &format!("{root}/groups"), &admin_token, &admin.csrf,
        json!({"id":inactive["id"],"revision":1,"name":"Changed","corp_id":"corp-test","chat_id":"changed","active":false})).await.0,409);
    // Offline fixture only: active groups in production must pass upstream validation.
    let _group = a
        .distribution_groups()
        .0
        .group(
            &admin,
            s,
            json!({"name":"Group","corp_id":"corp-test","chat_id":format!("chat-{suffix}")}),
        )
        .await
        .unwrap();
    let preview = ok(&app, "POST", &format!("{root}/template-previews"), &admin_token, &admin,
        json!({"revision":0,"name":"Daily","body":"{{school.name}}: {{post.title}}\n{{post.summary}}","note":"首次发布","sample_post_id":null,"publish":true})).await;
    let template = ok(
        &app,
        "POST",
        &format!("{root}/template-publications"),
        &admin_token,
        &admin,
        json!({"preview_id":preview["id"],"digest":preview["digest"]}),
    )
    .await;
    assert_eq!(template["published"], true);
    for path in [
        format!("{root}/delivery-drafts"),
        format!("{root}/delivery-attempts"),
    ] {
        assert_eq!(
            call(&app, "GET", &path, &admin_token, &admin.csrf, Value::Null)
                .await
                .0,
            404
        );
    }
    // School revocation is effective on the next request; no stale roles are embedded in cookies.
    a.wecom_workspaces()
        .grant(&admin, s, &format!("member{suffix}"), "revoked")
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/groups"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    let audit = ok(
        &app,
        "GET",
        "/api/v1/audit-events",
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    let events = audit.as_array().unwrap();
    assert!(events.iter().all(|event| event.get("school_id").is_none()));
    for action in ["group.save", "template.publish"] {
        assert!(events.iter().any(|event| event["action"] == action));
    }

    // A revoked member cannot restore access by rebinding the same school.
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/v1/wecom-distribution/workspaces/{s}/binding"),
            &token,
            &member.csrf,
            json!({})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/groups"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    let image = a
        .media
        .put(&admin, s, {
            use image::ImageEncoder;
            let mut bytes = Vec::new();
            image::codecs::png::PngEncoder::new(&mut bytes)
                .write_image(&[255, 255, 255, 255], 1, 1, image::ExtendedColorType::Rgba8)
                .unwrap();
            bytes
        })
        .await
        .unwrap();
    let asset = image.rsplit('/').next().unwrap().parse().unwrap();
    assert!(a.media.get(&admin, b, asset).await.is_err());
    assert!(
        a.media
            .put(&admin, s, b"<svg onload=alert(1)>".to_vec())
            .await
            .is_err()
    );
    let signed = a
        .legacy
        .signed_media(s, &image, "https://example.test")
        .await
        .unwrap();
    let media_token = signed.rsplit('/').next().unwrap();
    assert_eq!(a.legacy.media(media_token).await.unwrap().0, "image/png");
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT to_regclass('legacy_id_map') IS NULL AND to_regclass('legacy_aliases') IS NULL"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    pool.close().await;
}
