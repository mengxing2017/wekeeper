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
fn id(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
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
        401
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/schools",
            &admin_token,
            "",
            json!({"name":"Bad"})
        )
        .await
        .0,
        403
    );
    let school = ok(
        &app,
        "POST",
        "/api/v1/schools",
        &admin_token,
        &admin,
        json!({"name":"School A"}),
    )
    .await;
    let s = id(&school);
    let b = id(&ok(
        &app,
        "POST",
        "/api/v1/schools",
        &admin_token,
        &admin,
        json!({"name":"School B"}),
    )
    .await);
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
    a.schools
        .grant(&admin, s, memberid, "member")
        .await
        .unwrap();
    let (token, member) = a
        .identity
        .0
        .session(memberid, "browser", false)
        .await
        .unwrap();
    let root = format!("/api/v1/schools/{s}");
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/v1/schools/{b}/posts"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    let post = ok(
        &app,
        "POST",
        &format!("{root}/posts"),
        &token,
        &member,
        json!({"title":"Campus","content":"Hello","images":[],"tags":[]}),
    )
    .await;
    let p = id(&post);
    assert_eq!(post["audit_status"], "pending");
    let mut normalized = post.clone();
    for (key, value) in [
        ("id", "00000000-0000-0000-0000-000000000001"),
        ("school_id", "00000000-0000-0000-0000-000000000002"),
        ("author_id", "00000000-0000-0000-0000-000000000003"),
        ("created_at", "2026-10-21T00:00:00+00:00"),
        ("updated_at", "2026-10-21T00:00:00+00:00"),
    ] {
        normalized[key] = json!(value);
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/post-created.json"
    ))
    .unwrap();
    use axum::response::IntoResponse;
    let response = http::jsonapi::resource("posts", normalized).into_response();
    let wire: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(wire, fixture);

    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/posts/{p}/moderation-decisions"),
            &token,
            &member.csrf,
            json!({"revision":1,"decision":"approved"})
        )
        .await
        .0,
        403
    );
    let approved = ok(
        &app,
        "POST",
        &format!("{root}/posts/{p}/moderation-decisions"),
        &admin_token,
        &admin,
        json!({"revision":1,"decision":"approved","reason":"OK"}),
    )
    .await;
    assert_eq!(approved["revision"], 2);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/posts/{p}/moderation-decisions"),
            &admin_token,
            &admin.csrf,
            json!({"revision":1,"decision":"approved"})
        )
        .await
        .0,
        409
    );
    for kind in ["like", "collect"] {
        for _ in 0..2 {
            ok(
                &app,
                "PUT",
                &format!("{root}/posts/{p}/reactions/{kind}"),
                &token,
                &member,
                Value::Null,
            )
            .await;
        }
    }
    let reactions = ok(
        &app,
        "GET",
        &format!("{root}/reactions"),
        &token,
        &member,
        Value::Null,
    )
    .await;
    assert_eq!(reactions.as_array().unwrap().len(), 2);
    let comment = ok(
        &app,
        "POST",
        &format!("{root}/posts/{p}/comments"),
        &token,
        &member,
        json!({"content":"Comment"}),
    )
    .await;
    assert_eq!(comment["audit_status"], "pending");
    ok(
        &app,
        "POST",
        &format!("{root}/comments/{}/moderation-decisions", id(&comment)),
        &admin_token,
        &admin,
        json!({"revision":1,"decision":"approved"}),
    )
    .await;
    ok(
        &app,
        "GET",
        &format!("{root}/posts/{p}"),
        &token,
        &member,
        Value::Null,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM delivery_attempts WHERE school_id=$1")
            .bind(s)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let case = ok(
        &app,
        "POST",
        &format!("{root}/cases"),
        &token,
        &member,
        json!({"kind":"report","post_id":p,"content":"Report"}),
    )
    .await;
    ok(
        &app,
        "PUT",
        &format!("{root}/cases/{}", id(&case)),
        &admin_token,
        &admin,
        json!({"revision":1,"state":"resolved","resolution":"Reviewed"}),
    )
    .await;
    let preview = ok(&app,"POST",&format!("{root}/resource-previews"),&admin_token,&admin,
        json!({"kind":"blocked_word","title":"forbiddenword","revision":0,"active":true,"image_ref":null,"starts_at":null,"ends_at":null})).await;
    let resource = ok(
        &app,
        "POST",
        &format!("{root}/resource-publications"),
        &admin_token,
        &admin,
        json!({"preview_id":preview["id"],"digest":preview["digest"]}),
    )
    .await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/posts"),
            &token,
            &member.csrf,
            json!({"title":"Bad","content":"forbiddenword","images":[],"tags":[]})
        )
        .await
        .0,
        422
    );
    let visible = ok(
        &app,
        "GET",
        &format!("{root}/resources"),
        &token,
        &member,
        Value::Null,
    )
    .await;
    assert!(
        !visible
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == resource["id"])
    );
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
    let inactive = a.schools.0.group(&admin,s,json!({"name":"Inactive","corp_id":"corp-test","chat_id":format!("inactive-{suffix}"),"active":false})).await.unwrap();
    assert_eq!(call(&app, "POST", &format!("{root}/groups"), &admin_token, &admin.csrf,
        json!({"id":inactive["id"],"revision":1,"name":"Changed","corp_id":"corp-test","chat_id":"changed","active":false})).await.0,409);
    // Offline fixture only: active groups in production must pass upstream validation.
    let _group = a
        .schools
        .0
        .group(
            &admin,
            s,
            json!({"name":"Group","corp_id":"corp-test","chat_id":format!("chat-{suffix}")}),
        )
        .await
        .unwrap();
    let preview = ok(&app, "POST", &format!("{root}/template-previews"), &admin_token, &admin,
        json!({"revision":0,"name":"Daily","body":"{{school.name}}: {{post.title}}\n{{post.summary}}","note":"首次发布","sample_post_id":p,"publish":true})).await;
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
    a.schools
        .grant(&admin, s, memberid, "revoked")
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/posts"),
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
        &format!("{root}/audit"),
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    assert!(audit.as_array().unwrap().len() > 5);

    // A revoked member cannot restore access by rebinding the same school.
    a.schools.bind(&member, s).await.unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/posts"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    let preview = ok(&app,"POST","/api/v1/platform/resource-previews",&admin_token,&admin,
        json!({"kind":"legal","title":"Agreement","content":"Text","revision":0,"active":true,"image_ref":null,"starts_at":null,"ends_at":null})).await;
    let global = ok(
        &app,
        "POST",
        "/api/v1/platform/resource-publications",
        &admin_token,
        &admin,
        json!({"preview_id":preview["id"],"digest":preview["digest"]}),
    )
    .await;
    let resources = ok(
        &app,
        "GET",
        &format!("{root}/resources"),
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    assert!(
        resources
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == global["id"])
    );
    let image = a
        .media
        .put(&admin, s, b"\x89PNG\r\n\x1a\nimage-test".to_vec())
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
    let public_id = a.legacy.alias("fa_discover", p).await.unwrap();
    assert_eq!(
        a.legacy
            .resolve("fa_discover", &public_id.to_string())
            .await
            .unwrap(),
        p
    );
    // Former hard-limited feeds must execute real paged SQL and serialize resources.
    for feed in ["cases", "notifications"] {
        let (status, document) = call(
            &app,
            "GET",
            &format!("{root}/{feed}?page[size]=1"),
            &admin_token,
            &admin.csrf,
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{feed}: {document}");
        assert!(document["data"].is_array());
        assert_eq!(document["meta"]["page"]["size"], 1);
        assert!(document["meta"]["page"]["has_more"].is_boolean());
    }
    let (mini_token, _) = a
        .identity
        .0
        .session(adminid, "miniapp", false)
        .await
        .unwrap();
    // Native miniapp clients use Bearer sessions without browser Origin/CSRF.
    for (credential, cookie, expected) in [
        (mini_token.as_str(), false, 200),
        (admin_token.as_str(), false, 401),
        (mini_token.as_str(), true, 401),
    ] {
        let mut request = Request::builder().uri("/api/v1/auth/session");
        request = if cookie {
            request.header("cookie", format!("wekeeper_session={credential}"))
        } else {
            request.header("authorization", format!("Bearer {credential}"))
        };
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
    }
    let image_bytes = b"\x89PNG\r\n\x1a\nminiapp-raw-bytes";
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{root}/media"))
                .header("authorization", format!("Bearer {mini_token}"))
                .header("content-type", "application/octet-stream")
                .body(Body::from(image_bytes.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let document: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    let media_path = document["data"]["attributes"]["url"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(media_path)
                .header("authorization", format!("Bearer {mini_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(
        to_bytes(response.into_body(), 8192).await.unwrap().as_ref(),
        image_bytes
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/discover/User/schoolList")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("token", &mini_token)
                .body(Body::from("keyword=School"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let legacy: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(legacy["errors"][0]["code"], "not_found");
    assert!(legacy.get("code").is_none());
    pool.close().await;
}
