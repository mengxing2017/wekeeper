use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;
use wekeeper_api::{bootstrap, business::Actor, database, http};
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
        .header("content-type", "application/json");
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
                Body::from(body.to_string())
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
    v["data"].clone()
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
    let a = bootstrap::build_application(pool.clone());
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
    assert_eq!(normalized, fixture);

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
    let resource = ok(
        &app,
        "POST",
        &format!("{root}/resources"),
        &admin_token,
        &admin,
        json!({"kind":"blocked_word","title":"forbiddenword","revision":0}),
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
    let group = ok(
        &app,
        "POST",
        &format!("{root}/groups"),
        &admin_token,
        &admin,
        json!({"name":"Group","corp_id":"corp-test","chat_id":format!("chat-{suffix}")}),
    )
    .await;
    let template = ok(
        &app,
        "POST",
        &format!("{root}/templates"),
        &admin_token,
        &admin,
        json!({"name":"Daily"}),
    )
    .await;
    let version = ok(
        &app,
        "POST",
        &format!("{root}/templates/{}/versions", id(&template)),
        &admin_token,
        &admin,
        json!({"body":"{{school.name}}: {{post.title}}\n{{post.summary}}"}),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("{root}/template-versions/{}/publish", id(&version)),
        &admin_token,
        &admin,
        json!({"revision":1}),
    )
    .await;
    let draft_input =
        json!({"group_id":id(&group),"template_version_id":id(&version),"post_ids":[p]});
    let draft = ok(
        &app,
        "POST",
        &format!("{root}/delivery-drafts"),
        &admin_token,
        &admin,
        draft_input.clone(),
    )
    .await;
    assert_eq!(draft["content"], "School A: Campus\nHello");
    let d = id(&draft);
    let chat = group["chat_id"].as_str().unwrap();
    // HTTP cannot impersonate a verified enterprise identity, even with a platform admin session.
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/delivery-drafts/{d}/attempts"),
            &admin_token,
            &admin.csrf,
            json!({"current_chat_id":chat})
        )
        .await
        .0,
        403
    );
    let attempt = a
        .distribution
        .0
        .attempt(&admin, s, d, "first", chat, None, "")
        .await
        .unwrap();
    let at = id(&attempt);
    a.distribution.0.claim(&admin, s, at).await.unwrap();
    assert!(
        a.distribution.0.claim(&admin, s, at).await.is_err(),
        "one attempt cannot invoke SDK twice"
    );

    assert_eq!(
        a.distribution
            .0
            .attempt(&admin, s, d, "first", chat, None, "")
            .await
            .unwrap()["id"],
        attempt["id"]
    );
    assert!(
        a.distribution
            .0
            .attempt(&admin, s, d, "second", chat, None, "")
            .await
            .is_err()
    );
    let other = ok(
        &app,
        "POST",
        &format!("{root}/delivery-drafts"),
        &admin_token,
        &admin,
        draft_input.clone(),
    )
    .await;
    assert!(
        a.distribution
            .0
            .attempt(&admin, s, id(&other), "duplicate-draft", chat, None, "")
            .await
            .is_err()
    );
    sqlx::query("UPDATE delivery_attempts SET created_at=now()-interval '3 minutes' WHERE id=$1")
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    let history = a.distribution.0.attempts(s).await.unwrap();
    assert_eq!(history[0]["state"], "unknown");
    assert!(
        a.distribution
            .0
            .attempt(&admin, s, d, "resend-unreconciled", chat, Some(at), "retry")
            .await
            .is_err()
    );
    ok(
        &app,
        "POST",
        &format!("{root}/delivery-attempts/{at}/reconciliations"),
        &admin_token,
        &admin,
        json!({"decision":"operator_rejected","reason":"Manually checked the group"}),
    )
    .await;
    let late = ok(
        &app,
        "PUT",
        &format!("{root}/delivery-attempts/{at}/client-result"),
        &admin_token,
        &admin,
        json!({"state":"client_reported_success"}),
    )
    .await;
    assert_eq!(late["reconciliation"], "operator_rejected");
    assert_eq!(late["state"], "client_reported_success");
    ok(
        &app,
        "PUT",
        &format!("{root}/delivery-attempts/{at}/client-result"),
        &admin_token,
        &admin,
        json!({"state":"client_reported_success"}),
    )
    .await;
    let again = a
        .distribution
        .0
        .attempt(
            &admin,
            s,
            d,
            "explicit-resend",
            chat,
            Some(at),
            "Confirmed another share is needed",
        )
        .await
        .unwrap();
    assert_ne!(again["id"], attempt["id"]);
    ok(
        &app,
        "PUT",
        &format!("{root}/posts/{p}/visibility"),
        &admin_token,
        &admin,
        json!({"revision":2,"visibility":"hidden","top_sort":0}),
    )
    .await;
    assert!(
        a.distribution
            .0
            .attempt(&admin, s, d, "stale", chat, Some(id(&again)), "no")
            .await
            .is_err()
    );
    assert!(
        call(
            &app,
            "POST",
            &format!("{root}/delivery-drafts"),
            &admin_token,
            &admin.csrf,
            draft_input
        )
        .await
        .0 >= 400
    );
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
    let global = ok(
        &app,
        "POST",
        "/api/v1/platform/resources",
        &admin_token,
        &admin,
        json!({"kind":"legal","title":"Agreement","content":"Text","revision":0}),
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
    let (mini_token, _) = a
        .identity
        .0
        .session(adminid, "miniapp", false)
        .await
        .unwrap();
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
    assert_eq!(response.status(), 200);
    let legacy: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(legacy["code"], 1);
    assert!(legacy["data"]["list"][0]["id"].is_i64());
    pool.close().await;
}
