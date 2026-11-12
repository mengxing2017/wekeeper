//! Run only against WEKEEPER_TEST_DATABASE_URL with WEKEEPER_SOURCE_MASTER_KEY_V1 configured.
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;
use wekeeper_api::{
    bootstrap::build_application,
    business::Actor,
    http::app_router,
    source_access::{domain::*, north::local::Service, south::adapter::postgres::Repository},
};
fn actor(id: Uuid) -> Actor {
    Actor {
        id,
        username: "test".into(),
        display_name: "test".into(),
        platform_admin: true,
        bound_school_id: None,
        grants: vec![],
        csrf: String::new(),
        session_hash: String::new(),
        request_id: Uuid::new_v4().to_string(),
        browser: false,
        wecom_authenticated: false,
        wecom_binding_revision: 0,
    }
}
fn signed(
    key: &str,
    material: &str,
    path: &str,
    method: &str,
    id: Uuid,
    nonce: Uuid,
    body: &[u8],
) -> Request<Body> {
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signature = sign(
        &secret(material).unwrap(),
        &canonical(
            key,
            method,
            path,
            &timestamp,
            &nonce.to_string(),
            &id.to_string(),
            body,
        ),
    );
    Request::builder()
        .method(method)
        .uri(path)
        .header("accept", MEDIA_TYPE)
        .header("content-type", MEDIA_TYPE)
        .header("x-source-version", "2")
        .header("x-source-key-id", key)
        .header("x-source-timestamp", timestamp)
        .header("x-source-nonce", nonce.to_string())
        .header("idempotency-key", id.to_string())
        .header("x-source-signature", signature)
        .body(Body::from(body.to_vec()))
        .unwrap()
}
async fn read(r: axum::response::Response) -> (u16, Value) {
    let status = r.status().as_u16();
    (
        status,
        serde_json::from_slice(&to_bytes(r.into_body(), MAX_BODY).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL and source encryption key"]
async fn two_instances_replay_scope_lifetime_and_deletion() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated test database required");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'test',true)",
    )
    .bind(id)
    .bind(id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    let a = actor(id);
    let svc = Service(pool.clone());
    let key=svc.manage(&a,"source-credentials",None,"POST",Some(serde_json::from_value(json!({"unit_name":"unit","remark":"credential remark","status":"active","valid_from":chrono::Utc::now()-chrono::Duration::hours(1),"valid_until":null})).unwrap()),1,"","",None).await.unwrap();
    assert!(key.get("source_id").is_none());
    let key = key["id"].as_str().unwrap();
    let source = Repository(pool.clone())
        .caller_for_key(key)
        .await
        .unwrap()
        .to_string();
    // No separate caller/platform creation or host configuration is required.
    assert!(
        svc.manage(&a, "source-platforms", None, "GET", None, 1, "", "", None)
            .await
            .is_err()
    );
    let route=svc.manage(&a,"source-routes",None,"POST",Some(serde_json::from_value(json!({"key_id":key,"business_unit_name":"M1","remark":"route remark","status":"active"})).unwrap()),1,"","",None).await.unwrap();
    let created_unit = Uuid::parse_str(route["business_unit_id"].as_str().unwrap()).unwrap();
    let route = route["id"].as_str().unwrap();
    let listed = svc
        .manage(
            &a,
            "source-routes",
            Some(route),
            "GET",
            None,
            1,
            "",
            "",
            None,
        )
        .await
        .unwrap();
    assert_eq!(listed["items"][0]["business_unit_name"], "M1");
    let (unit_id, platform, unit_name): (Uuid, String, String) = sqlx::query_as(
        "SELECT u.id,u.platform_id::text,u.name FROM business_units u JOIN source_routes r ON r.business_unit_id=u.id WHERE r.route_id=$1",
    )
    .bind(route)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(platform, source);
    assert_eq!(unit_name, "M1");
    assert_eq!(created_unit, unit_id);
    assert_eq!(listed["items"][0]["business_unit_id"], unit_id.to_string());
    assert_ne!(unit_id.to_string(), route);
    let duplicate = sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) SELECT $1,source_id,business_unit_id,'duplicate','active' FROM source_routes WHERE route_id=$2")
        .bind(Uuid::new_v4().to_string()).bind(route).execute(&pool).await;
    assert!(duplicate.is_err());
    let associations = Repository(pool.clone())
        .associations(&a, key, 1)
        .await
        .unwrap();
    assert_eq!(associations["items"][0]["route_id"], route);
    assert_eq!(associations["items"][0]["status"], "active");
    let material = Repository(pool.clone())
        .deliver(key, "isolated-test")
        .await
        .unwrap();
    assert!(
        Repository(pool.clone())
            .deliver(key, "isolated-test")
            .await
            .is_err()
    );
    for _ in 0..2 {
        let revealed = Repository(pool.clone())
            .reveal_secret(&a, key)
            .await
            .unwrap();
        assert_eq!(revealed.secret, material);
        assert_eq!(revealed.key_id, key);
    }
    let mut forbidden = a.clone();
    forbidden.platform_admin = false;
    assert_eq!(
        Repository(pool.clone())
            .reveal_secret(&forbidden, key)
            .await
            .err()
            .unwrap()
            .0,
        "forbidden"
    );
    let audit: Vec<Value> = sqlx::query_scalar("SELECT detail FROM source_access_audit WHERE key_id=$1 AND action='source.credential.secret_reveal'").bind(key).fetch_all(&pool).await.unwrap();
    assert_eq!(audit.len(), 2);
    assert!(!serde_json::to_string(&audit).unwrap().contains(&material));
    let one = app_router(build_application(pool.clone()));
    let two = app_router(build_application(pool.clone()));
    let event = Uuid::now_v7();
    let body=serde_json::to_vec(&json!({"data":{"type":"source-events","id":event,"attributes":{"event_id":event,"schema_version":2,"route_id":route,"event_type":"content.upsert","external_content_id":"post-1","version":"1","sequence":1,"occurred_at":chrono::Utc::now(),"title":"中文🌸","content":"内容","detail_url":"https://content.example.net/articles/1"}}})).unwrap();
    let nonce = Uuid::new_v4();
    let path = "/api/v2/source-events";
    let (r1, r2) = tokio::join!(
        one.clone()
            .oneshot(signed(key, &material, path, "POST", event, nonce, &body)),
        two.clone()
            .oneshot(signed(key, &material, path, "POST", event, nonce, &body))
    );
    let (s1, v1) = read(r1.unwrap()).await;
    let (s2, v2) = read(r2.unwrap()).await;
    assert!(
        (s1 == 202 && s2 == 409) || (s2 == 202 && s1 == 409),
        "{s1} {v1} {s2} {v2}"
    );
    let receipt = if s1 == 202 {
        v1["data"]["id"].clone()
    } else {
        v2["data"]["id"].clone()
    };
    let accepted_unit: Uuid =
        sqlx::query_scalar("SELECT business_unit_id FROM source_receipts WHERE receipt_id=$1")
            .bind(Uuid::parse_str(receipt.as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(accepted_unit, unit_id);
    let lookup = wekeeper_api::source_access::north::local::AcceptedIdentityService(Repository(
        pool.clone(),
    ));
    let identity = lookup
        .find(
            source.parse().unwrap(),
            Uuid::parse_str(receipt.as_str().unwrap()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(identity.platform_id.to_string(), source);
    assert_eq!(identity.unit_id, unit_id);
    let denied = lookup
        .find(
            Uuid::new_v4(),
            Uuid::parse_str(receipt.as_str().unwrap()).unwrap(),
        )
        .await;
    assert!(matches!(denied, Err(Fault("receipt_not_found"))));
    let matcher = wekeeper_api::source_access::north::local::ad_targeting::ScopeMatcher(
        Repository(pool.clone()),
    );
    let platform_id = source.parse::<Uuid>().unwrap();
    let receipt_id = Uuid::parse_str(receipt.as_str().unwrap()).unwrap();
    use wekeeper_api::message_orchestration::{
        domain::EventKind, north::local::MessageIntake,
        south::adapter::postgres::Repository as MessageRepository,
    };
    use wekeeper_api::source_access::north::local::orchestration_intake::{
        AcceptedMessageBridge, BridgeError,
    };
    let bridge = AcceptedMessageBridge {
        receipts: Repository(pool.clone()),
        messages: MessageIntake(MessageRepository(pool.clone())),
    };
    // Ordinary writes cannot alter the accepted bytes or their digest.
    assert!(
        sqlx::query("UPDATE source_inbox SET request_body=$2 WHERE receipt_id=$1")
            .bind(receipt_id)
            .bind(b"tampered".as_slice())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE source_receipts SET body_sha256=$2 WHERE receipt_id=$1")
            .bind(receipt_id)
            .bind("0".repeat(64))
            .execute(&pool)
            .await
            .is_err()
    );
    // Simulate damaged storage by bypassing just this guard in the isolated test database.
    let mut damaged: Value = serde_json::from_slice(&body).unwrap();
    damaged["data"]["attributes"]["content"] = json!("tampered");
    sqlx::query("ALTER TABLE source_inbox DISABLE TRIGGER source_inbox_body_immutable")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE source_inbox SET request_body=$2 WHERE receipt_id=$1")
        .bind(receipt_id)
        .bind(serde_json::to_vec(&damaged).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE source_inbox ENABLE TRIGGER source_inbox_body_immutable")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        bridge.accept(platform_id, receipt_id).await,
        Err(BridgeError::Source(Fault("evidence_mismatch")))
    ));
    let missing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_messages WHERE platform_id=$1 AND evidence_ref=$2",
    )
    .bind(platform_id)
    .bind(receipt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(missing, 0);
    let damaged_worker =
        wekeeper_api::source_access::north::local::orchestration_intake::HandoffWorker {
            queue: Repository(pool.clone()),
            bridge: AcceptedMessageBridge {
                receipts: Repository(pool.clone()),
                messages: MessageIntake(MessageRepository(pool.clone())),
            },
        };
    damaged_worker.run_once().await.unwrap();
    let failure: (String, Option<String>) =
        sqlx::query_as("SELECT state,last_error FROM source_message_handoffs WHERE receipt_id=$1")
            .bind(receipt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        failure,
        ("pending".into(), Some("evidence_mismatch".into()))
    );
    sqlx::query("ALTER TABLE source_inbox DISABLE TRIGGER source_inbox_body_immutable")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE source_inbox SET request_body=$2 WHERE receipt_id=$1")
        .bind(receipt_id)
        .bind(&body)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE source_inbox ENABLE TRIGGER source_inbox_body_immutable")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE source_message_handoffs SET next_attempt_at=clock_timestamp() WHERE receipt_id=$1",
    )
    .bind(receipt_id)
    .execute(&pool)
    .await
    .unwrap();
    let accepted = bridge.accept(platform_id, receipt_id).await.unwrap();
    assert_eq!(accepted.facts.unit_id, unit_id);
    assert_eq!(accepted.facts.source_event_id, event);
    assert_eq!(accepted.facts.original.event_kind, EventKind::Upsert);
    assert_eq!(accepted.facts.original.external_content_id, "post-1");
    assert_eq!(accepted.facts.original.sequence, 1);
    assert_eq!(accepted.facts.original.content.as_deref(), Some("内容"));
    assert_eq!(
        bridge.accept(platform_id, receipt_id).await.unwrap().id,
        accepted.id
    );
    assert!(matches!(
        bridge.accept(Uuid::new_v4(), receipt_id).await,
        Err(BridgeError::Source(Fault("receipt_not_found")))
    ));
    use wekeeper_api::source_access::north::local::orchestration_intake::HandoffWorker;
    use wekeeper_api::source_access::south::port::MessageHandoffStore;
    let queue = Repository(pool.clone());
    let stale_claim = queue.claim().await.unwrap().unwrap();
    assert_eq!(stale_claim.receipt_id, receipt_id);
    sqlx::query("UPDATE source_message_handoffs SET claim_until=clock_timestamp()-interval '1 second' WHERE receipt_id=$1")
        .bind(receipt_id).execute(&pool).await.unwrap();
    let worker = || HandoffWorker {
        queue: Repository(pool.clone()),
        bridge: AcceptedMessageBridge {
            receipts: Repository(pool.clone()),
            messages: MessageIntake(MessageRepository(pool.clone())),
        },
    };
    let worker_one = worker();
    let worker_two = worker();
    let (first, second) = tokio::join!(worker_one.run_once(), worker_two.run_once());
    first.unwrap();
    second.unwrap();
    queue.complete(&stale_claim, Uuid::new_v4()).await.unwrap();
    let handoff: (String, Uuid, i32) = sqlx::query_as(
        "SELECT state,message_id,attempts FROM source_message_handoffs WHERE receipt_id=$1",
    )
    .bind(receipt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(handoff.0, "complete");
    assert_eq!(handoff.1, accepted.id);
    assert_eq!(handoff.2, 3);
    use wekeeper_api::advertising::domain::targeting::TargetScope;
    let scopes = [
        TargetScope::Global {},
        TargetScope::Platform { platform_id },
        TargetScope::Unit {
            platform_id,
            unit_id,
        },
        TargetScope::Unit {
            platform_id,
            unit_id: Uuid::new_v4(),
        },
        TargetScope::Platform {
            platform_id: Uuid::new_v4(),
        },
    ];
    assert_eq!(
        matcher
            .matching_indices(platform_id, receipt_id, &scopes)
            .await
            .unwrap(),
        vec![0, 1, 2]
    );
    assert!(matches!(
        matcher
            .matching_indices(Uuid::new_v4(), receipt_id, &scopes)
            .await,
        Err(Fault("receipt_not_found"))
    ));
    // Discovery uses the frozen business identity, while the advertising
    // repository itself does not inspect the source route.
    let ad_material = Uuid::new_v4();
    let material_version = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
        .bind(ad_material)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,created_by) VALUES($1,$2,1,'素材','正文',$3)")
        .bind(material_version).bind(ad_material).bind(id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
        .bind(ad_material)
        .bind(material_version)
        .execute(&pool)
        .await
        .unwrap();
    let global_rule = Uuid::new_v4();
    let platform_rule = Uuid::new_v4();
    let unit_rule = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_placement_rules(id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,created_by,updated_by) VALUES($1,'global',$4,'global',NULL,NULL,true,now()-interval '1 hour',$7,$7),($2,'platform',$4,'platform',$5,NULL,true,now()-interval '1 hour',$7,$7),($3,'unit',$4,'unit',$5,$6,true,now()-interval '1 hour',$7,$7)")
        .bind(global_rule).bind(platform_rule).bind(unit_rule).bind(ad_material).bind(platform_id).bind(unit_id).bind(id).execute(&pool).await.unwrap();
    let discovery = wekeeper_api::source_access::north::local::ad_targeting::CandidateDiscovery {
        identities: Repository(pool.clone()),
        rules: wekeeper_api::advertising::north::local::MatchingRules(
            wekeeper_api::advertising::south::adapter::postgres::Repository(pool.clone()),
        ),
    };
    let matches = discovery
        .page(platform_id, receipt_id, chrono::Utc::now(), None)
        .await
        .unwrap();
    let matched: std::collections::HashSet<_> =
        matches.items.iter().map(|item| item.rule_id).collect();
    assert!(
        [global_rule, platform_rule, unit_rule]
            .iter()
            .all(|id| matched.contains(id))
    );
    assert!(matches!(
        discovery
            .page(Uuid::new_v4(), receipt_id, chrono::Utc::now(), None)
            .await,
        Err(
            wekeeper_api::source_access::north::local::ad_targeting::DiscoveryError::Identity(
                Fault("receipt_not_found")
            )
        )
    ));
    let (status, dup) = read(
        two.clone()
            .oneshot(signed(
                key,
                &material,
                path,
                "POST",
                event,
                Uuid::new_v4(),
                &body,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(dup["data"]["id"], receipt);
    // Distinct nonces must still converge on one receipt across server instances.
    let concurrent_event = Uuid::now_v7();
    let mut concurrent_body: Value = serde_json::from_slice(&body).unwrap();
    concurrent_body["data"]["id"] = json!(concurrent_event);
    concurrent_body["data"]["attributes"]["event_id"] = json!(concurrent_event);
    concurrent_body["data"]["attributes"]["external_content_id"] = json!("parallel-event");
    let concurrent_body = serde_json::to_vec(&concurrent_body).unwrap();
    let (left, right) = tokio::join!(
        one.clone().oneshot(signed(
            key,
            &material,
            path,
            "POST",
            concurrent_event,
            Uuid::new_v4(),
            &concurrent_body
        )),
        two.clone().oneshot(signed(
            key,
            &material,
            path,
            "POST",
            concurrent_event,
            Uuid::new_v4(),
            &concurrent_body
        ))
    );
    let (left_status, left) = read(left.unwrap()).await;
    let (right_status, right) = read(right.unwrap()).await;
    assert!(
        (left_status == 202 && right_status == 200) || (left_status == 200 && right_status == 202),
        "{left_status} {left} {right_status} {right}"
    );
    assert_eq!(left["data"]["id"], right["data"]["id"]);
    let mut changed: Value = serde_json::from_slice(&body).unwrap();
    changed["data"]["attributes"]["title"] = json!("different");
    let changed = serde_json::to_vec(&changed).unwrap();
    assert_eq!(
        read(
            one.clone()
                .oneshot(signed(
                    key,
                    &material,
                    path,
                    "POST",
                    event,
                    Uuid::new_v4(),
                    &changed
                ))
                .await
                .unwrap()
        )
        .await
        .0,
        409
    );
    // A second independent platform cannot use the first platform's route or receipt.
    let other_source = Uuid::now_v7();
    let other_key = Uuid::now_v7().to_string();
    let other_secret = wekeeper_api::source_access::crypto::random_secret().unwrap();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
        .bind(other_source)
        .execute(&pool)
        .await
        .unwrap();
    let cipher = wekeeper_api::source_access::crypto::seal(&other_secret, &other_key).unwrap();
    sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encrypted_secret,encryption_version,status,valid_from,valid_until,created_by) VALUES($1,$2,'P2',$3,'v1','active',now()-interval '1 hour',now()+interval '1 day',$4)").bind(&other_key).bind(other_source).bind(cipher).bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        read(
            two.clone()
                .oneshot(signed(
                    &other_key,
                    &other_secret,
                    path,
                    "POST",
                    event,
                    Uuid::new_v4(),
                    &body
                ))
                .await
                .unwrap()
        )
        .await
        .0,
        403
    );
    let foreign_get = format!("/api/v2/source-receipts/{}", receipt.as_str().unwrap());
    assert_eq!(
        read(
            two.clone()
                .oneshot(signed(
                    &other_key,
                    &other_secret,
                    &foreign_get,
                    "GET",
                    event,
                    Uuid::new_v4(),
                    b""
                ))
                .await
                .unwrap()
        )
        .await
        .0,
        404
    );
    let get = format!("/api/v2/source-receipts/{}", receipt.as_str().unwrap());
    // A perpetual credential can authenticate and read the receipt.
    assert_eq!(
        read(
            one.clone()
                .oneshot(signed(
                    key,
                    &material,
                    &get,
                    "GET",
                    event,
                    Uuid::new_v4(),
                    b""
                ))
                .await
                .unwrap()
        )
        .await
        .0,
        200
    );
    // Exercise the actual signed POST -> durable inbox -> worker -> signed GET path.
    use wekeeper_api::source_access::south::adapter::postgres::routing::RoutingRepository;
    let worker = RoutingRepository(pool.clone());
    worker.run_once().await.unwrap();
    let (_, state) = read(
        one.clone()
            .oneshot(signed(
                key,
                &material,
                &get,
                "GET",
                event,
                Uuid::new_v4(),
                b"",
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(state["data"]["attributes"]["status"], "applied");
    assert_eq!(
        state["data"]["attributes"]["processing_stage"],
        "configuration_wait"
    );
    assert_eq!(state["data"]["attributes"]["retryable"], false);
    assert_eq!(state["data"]["attributes"]["observed_sequence"], 1);
    let conflicts:Value=sqlx::query_scalar("SELECT detail FROM source_access_audit WHERE source_id=$1 AND action='source.event.conflict' LIMIT 1").bind(source.parse::<Uuid>().unwrap()).fetch_one(&pool).await.unwrap();
    assert_ne!(conflicts["first_digest"], conflicts["conflicting_digest"]);
    for (seq, title, expected) in [
        (1, "中文🌸", "applied"),
        (1, "另一份正文", "rejected"),
        (6, "更高版本", "applied"),
        (5, "迟到旧版", "ignored_stale"),
    ] {
        let eid = Uuid::new_v4();
        let mut payload: Value = serde_json::from_slice(&body).unwrap();
        payload["data"]["id"] = json!(eid);
        payload["data"]["attributes"]["event_id"] = json!(eid);
        payload["data"]["attributes"]["sequence"] = json!(seq);
        payload["data"]["attributes"]["title"] = json!(title);
        let bytes = serde_json::to_vec(&payload).unwrap();
        let (status, accepted) = read(
            two.clone()
                .oneshot(signed(
                    key,
                    &material,
                    path,
                    "POST",
                    eid,
                    Uuid::new_v4(),
                    &bytes,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, 202, "{accepted}");
        worker.run_once().await.unwrap();
        let getpath = format!(
            "/api/v2/source-receipts/{}",
            accepted["data"]["id"].as_str().unwrap()
        );
        let response = one
            .clone()
            .oneshot(signed(
                key,
                &material,
                &getpath,
                "GET",
                eid,
                Uuid::new_v4(),
                b"",
            ))
            .await
            .unwrap();
        assert_eq!(response.headers()["x-source-poll-after"], "5");
        let (status, projected) = read(response).await;
        assert_eq!(status, 200);
        assert_eq!(
            projected["data"]["attributes"]["status"], expected,
            "{projected}"
        );
    }
    // SQL failure between receipt and inbox insertion must roll back the complete acceptance.
    sqlx::query("CREATE FUNCTION source_test_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'isolated failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER source_test_fail BEFORE INSERT ON source_inbox FOR EACH ROW EXECUTE FUNCTION source_test_fail()").execute(&pool).await.unwrap();
    let failed_event = Uuid::new_v4();
    let mut payload: Value = serde_json::from_slice(&body).unwrap();
    payload["data"]["id"] = json!(failed_event);
    payload["data"]["attributes"]["event_id"] = json!(failed_event);
    let bytes = serde_json::to_vec(&payload).unwrap();
    let (status, _) = read(
        one.clone()
            .oneshot(signed(
                key,
                &material,
                path,
                "POST",
                failed_event,
                Uuid::new_v4(),
                &bytes,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, 503);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM source_receipts WHERE source_id=$1 AND event_id=$2",
    )
    .bind(source.parse::<Uuid>().unwrap())
    .bind(failed_event)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER source_test_fail ON source_inbox")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION source_test_fail()")
        .execute(&pool)
        .await
        .unwrap();
    let (status, recovered) = read(
        one.clone()
            .oneshot(signed(
                key,
                &material,
                path,
                "POST",
                failed_event,
                Uuid::new_v4(),
                &bytes,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, 202, "{recovered}");
    let usage: i64 = sqlx::query_scalar(
        "SELECT sum(queries)::bigint FROM source_query_usage WHERE source_id=$1",
    )
    .bind(source.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(usage >= 6);
    sqlx::query("UPDATE source_query_usage SET queries=600 WHERE source_id=$1")
        .bind(source.parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let limited = one
        .clone()
        .oneshot(signed(
            key,
            &material,
            &get,
            "GET",
            event,
            Uuid::new_v4(),
            b"",
        ))
        .await
        .unwrap();
    assert_eq!(limited.status().as_u16(), 429);
    assert_eq!(limited.headers()["retry-after"], "60");

    let list = svc
        .manage(&a, "source-credentials", None, "GET", None, 1, "", "", None)
        .await
        .unwrap();
    assert!(!list.to_string().contains(&material));
    assert!(!list.to_string().contains("encrypted_secret"));
    svc.manage(
        &a,
        "source-credentials",
        Some(key),
        "DELETE",
        Some(serde_json::from_value(json!({})).unwrap()),
        1,
        "",
        "",
        None,
    )
    .await
    .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM source_credentials WHERE key_id=$1)+(SELECT count(*) FROM source_key_routes WHERE key_id=$1)+(SELECT count(*) FROM source_routes WHERE route_id=$2)+(SELECT count(*) FROM source_request_nonces WHERE key_id=$1)").bind(key).bind(route).fetch_one(&pool).await.unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(
        Repository(pool.clone())
            .reveal_secret(&a, key)
            .await
            .err()
            .unwrap()
            .0,
        "not_found"
    );
    assert_eq!(
        read(
            two.clone()
                .oneshot(signed(
                    key,
                    &material,
                    &get,
                    "GET",
                    event,
                    Uuid::new_v4(),
                    b""
                ))
                .await
                .unwrap()
        )
        .await
        .0,
        401
    );
    let rows = svc
        .manage(&a, "source-credentials", None, "GET", None, 1, "", "", None)
        .await
        .unwrap();
    assert!(
        !rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == key)
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE r.source_id=$1 AND r.event_id=$2").bind(source.parse::<Uuid>().unwrap()).bind(event).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    let mut unauthorized = a.clone();
    unauthorized.platform_admin = false;
    assert!(
        svc.manage(
            &unauthorized,
            "source-credentials",
            None,
            "GET",
            None,
            1,
            "",
            "",
            None
        )
        .await
        .is_err()
    );
}
