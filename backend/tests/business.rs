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
        "platform" => "resources",
        "schools" => match segments.get(4).copied().unwrap_or("schools") {
            "members" => "accounts",
            "posts" if segments.get(6) == Some(&"comments") => "comments",
            "templates" if segments.get(6) == Some(&"versions") => "template-versions",
            "delivery-drafts" if segments.get(6) == Some(&"attempts") => "delivery-attempts",
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
        if path.contains("/history") {
            json!({"items":items,"blocked":v["meta"]["blocked"],"latest_attempt_id":v["meta"]["latest_attempt_id"]})
        } else {
            json!(items)
        }
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
    let group = a
        .schools
        .0
        .group(
            &admin,
            s,
            json!({"name":"Group","corp_id":"corp-test","chat_id":format!("chat-{suffix}")}),
        )
        .await
        .unwrap();
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
    // Construct an existing legacy snapshot to exercise historical reads after retirement.
    let draft = a
        .distribution
        .draft(
            &admin,
            s,
            serde_json::from_value(draft_input.clone()).unwrap(),
            "http://localhost:5173",
        )
        .await
        .unwrap();
    assert_eq!(draft["content"], "School A: Campus\nHello");
    let d = id(&draft);
    let chat = group["chat_id"].as_str().unwrap();
    // The old write route is retired even for an authenticated platform administrator.
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
        422
    );
    let attempt = a
        .distribution
        .0
        .attempt(&admin, s, d, "first", chat, None, "")
        .await
        .unwrap();
    let at = id(&attempt);
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("{root}/delivery-attempts/{at}/client-result"),
            &admin_token,
            &admin.csrf,
            json!({"state":"client_reported_success"})
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("{root}/delivery-attempts/{at}/client-result"),
            &admin_token,
            &admin.csrf,
            json!({"state":"cancelled"})
        )
        .await
        .0,
        422
    );
    let mut stale_identity = admin.clone();
    stale_identity.wecom_binding_revision += 1;
    assert!(
        a.distribution
            .0
            .claim(&stale_identity, s, at)
            .await
            .is_err(),
        "a stale enterprise binding cannot claim a prepared attempt"
    );
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
    // Construct an existing legacy snapshot to exercise historical reads after retirement.
    let other = a
        .distribution
        .draft(
            &admin,
            s,
            serde_json::from_value(draft_input.clone()).unwrap(),
            "http://localhost:5173",
        )
        .await
        .unwrap();
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
    let history = a.distribution.0.attempts(s, 100, 0).await.unwrap();
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
    let old_detail = ok(
        &app,
        "GET",
        &format!("{root}/delivery-drafts/{d}"),
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    assert_eq!(old_detail["id"], draft["id"]);
    let related = ok(
        &app,
        "GET",
        &format!("{root}/delivery-drafts/{}/history?page[size]=1", id(&other)),
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    assert_eq!(related["blocked"], true);
    assert_eq!(related["latest_attempt_id"], again["id"]);
    assert_eq!(related["items"].as_array().unwrap().len(), 1);
    assert!(related["items"][0]["actor_name"].is_string());
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/delivery-drafts/{d}"),
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
            &format!("{root}/delivery-drafts/{d}/history"),
            &token,
            &member.csrf,
            Value::Null
        )
        .await
        .0,
        403
    );
    let (status, first_page) = call(
        &app,
        "GET",
        &format!("{root}/delivery-drafts?page[size]=1&page[number]=1"),
        &admin_token,
        &admin.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        first_page["meta"]["page"],
        json!({"number":1,"size":1,"has_more":true})
    );
    let page1 = &first_page["data"];
    let page2 = ok(
        &app,
        "GET",
        &format!("{root}/delivery-drafts?page[size]=1&page[number]=2"),
        &admin_token,
        &admin,
        Value::Null,
    )
    .await;
    assert_ne!(page1[0]["id"], page2[0]["id"]);
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{root}/delivery-attempts?page[number]=0"),
            &admin_token,
            &admin.csrf,
            Value::Null
        )
        .await
        .0,
        422
    );

    // A's unresolved result must block A+B even when B has a newer completed attempt.
    sqlx::query("UPDATE delivery_attempts SET state='unknown',created_at=now()-interval '4 minutes' WHERE id=$1").bind(id(&again)).execute(&pool).await.unwrap();
    let p_b = Uuid::now_v7();
    sqlx::query("INSERT INTO posts(id,school_id,author_id,title,content,audit_status,visibility) VALUES($1,$2,$3,'B','B','approved','visible')").bind(p_b).bind(s).bind(admin.id).execute(&pool).await.unwrap();
    let draft_b = a
        .distribution
        .0
        .draft(
            &admin,
            s,
            id(&group),
            id(&version),
            vec![p_b],
            "http://localhost:5173",
        )
        .await
        .unwrap();
    let attempt_b = a
        .distribution
        .0
        .attempt(&admin, s, id(&draft_b), "B-only", chat, None, "")
        .await
        .unwrap();
    a.distribution
        .0
        .claim(&admin, s, id(&attempt_b))
        .await
        .unwrap();
    a.distribution
        .0
        .result(
            &admin,
            s,
            id(&attempt_b),
            json!({"state":"client_reported_success"}),
        )
        .await
        .unwrap();
    let merged = a
        .distribution
        .0
        .draft(
            &admin,
            s,
            id(&group),
            id(&version),
            vec![p, p_b],
            "http://localhost:5173",
        )
        .await
        .unwrap();
    assert!(
        a.distribution
            .0
            .attempt(
                &admin,
                s,
                id(&merged),
                "merged-blocked",
                chat,
                Some(id(&attempt_b)),
                "again"
            )
            .await
            .is_err()
    );
    let related = a
        .distribution
        .0
        .related(s, id(&merged), 1, 0)
        .await
        .unwrap();
    assert!(related.blocked);
    assert_eq!(related.latest_attempt_id, Some(id(&attempt_b)));
    a.distribution
        .0
        .reconcile(
            &admin,
            s,
            id(&again),
            json!({"decision":"operator_rejected","reason":"checked A"}),
        )
        .await
        .unwrap();
    assert!(
        !a.distribution
            .0
            .related(s, id(&merged), 1, 0)
            .await
            .unwrap()
            .blocked
    );
    a.distribution
        .0
        .attempt(
            &admin,
            s,
            id(&merged),
            "merged-allowed",
            chat,
            Some(id(&attempt_b)),
            "checked all",
        )
        .await
        .unwrap();

    let p_c = Uuid::now_v7();
    sqlx::query("INSERT INTO posts(id,school_id,author_id,title,content,audit_status,visibility) VALUES($1,$2,$3,'C','C','approved','visible')").bind(p_c).bind(s).bind(admin.id).execute(&pool).await.unwrap();
    let c1 = a
        .distribution
        .0
        .draft(
            &admin,
            s,
            id(&group),
            id(&version),
            vec![p_c],
            "http://localhost:5173",
        )
        .await
        .unwrap();
    let c2 = a
        .distribution
        .0
        .draft(
            &admin,
            s,
            id(&group),
            id(&version),
            vec![p_c],
            "http://localhost:5173",
        )
        .await
        .unwrap();
    let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            a.distribution
                .0
                .attempt(&admin, s, id(&c1), "concurrent-left", chat, None, ""),
            a.distribution
                .0
                .attempt(&admin, s, id(&c2), "concurrent-right", chat, None, "")
        )
    })
    .await
    .expect("overlapping drafts must not deadlock");
    assert_eq!(
        usize::from(left.is_ok()) + usize::from(right.is_ok()),
        1,
        "only one overlapping draft may be prepared"
    );
    sqlx::query("INSERT INTO delivery_drafts(id,school_id,group_id,template_version_id,creator_id,content,content_hash,created_at) SELECT gen_random_uuid(),school_id,group_id,template_version_id,creator_id,content,content_hash,now()+interval '1 second' FROM delivery_drafts CROSS JOIN generate_series(1,101) WHERE id=$1").bind(d).execute(&pool).await.unwrap();
    let first = a.distribution.0.drafts(s, 100, 0).await.unwrap();
    assert!(!first.iter().any(|r| r["id"] == draft["id"]));
    assert_eq!(
        ok(
            &app,
            "GET",
            &format!("{root}/delivery-drafts/{d}"),
            &admin_token,
            &admin,
            Value::Null
        )
        .await["id"],
        draft["id"]
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/v1/schools/{b}/delivery-drafts/{d}"),
            &admin_token,
            &admin.csrf,
            Value::Null
        )
        .await
        .0,
        404
    );
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
