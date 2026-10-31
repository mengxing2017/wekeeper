use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::{Error, Page},
    database,
    distribution::{
        domain::mobile_delivery::*,
        south::{adapter::postgres::mobile_delivery::Repository, port::mobile_delivery::Store},
    },
};
struct Fixture {
    pool: PgPool,
    repo: Repository,
    actor: Uuid,
    school: Uuid,
    sender: Uuid,
    group: Uuid,
    device: Uuid,
    principal: Principal,
    binding_revision: i64,
    token: String,
}
fn uuid(v: &Value, key: &str) -> Uuid {
    Uuid::parse_str(v[key].as_str().unwrap()).unwrap()
}
fn command(v: Value) -> Command {
    serde_json::from_value(v).unwrap()
}
impl Fixture {
    async fn call(&self, school: Option<Uuid>, id: Option<Uuid>, v: Value) -> Value {
        self.repo
            .execute(
                Principal::Actor(self.actor),
                Scope { school, id },
                &Uuid::new_v4().to_string(),
                command(v),
                true,
                "https://example.test",
            )
            .await
            .unwrap()
    }
    async fn device_call(&self, id: Option<Uuid>, v: Value) -> Value {
        self.repo
            .execute(
                self.principal.clone(),
                Scope { school: None, id },
                &Uuid::new_v4().to_string(),
                command(v),
                true,
                "https://example.test",
            )
            .await
            .unwrap()
    }
    async fn task(&self, body: &str) -> Value {
        self.call(Some(self.school),None,json!({"operation":"draft","activity_id":"school-notice","items":[{"group_id":self.group,"sender_account_id":self.sender,"source":{"kind":"text","text_content":body}}]})).await
    }
    async fn confirm(&self, t: &Value) {
        let id = uuid(t, "id");
        let preview = self
            .call(
                Some(self.school),
                Some(id),
                json!({"operation":"preview","expected_revision":t["revision"]}),
            )
            .await;
        self.call(Some(self.school),Some(id),json!({"operation":"confirm","expected_revision":t["revision"],"preview_digest":preview["preview_digest"],"content_review":true,"scope_acceptance":true,"substantive_difference_reviewed":true,"evidence_ref":"review:1"})).await;
    }
    async fn prepared(&self, body: &str) -> (Value, Value) {
        let t = self.task(body).await;
        self.confirm(&t).await;
        let claim = self
            .device_call(
                None,
                json!({"operation":"claim","binding_revision":self.binding_revision}),
            )
            .await;
        assert!(claim["claim_id"].is_string(), "{claim}");
        let prep=self.device_call(Some(uuid(&claim,"id")),json!({"operation":"prepare","fence":claim["fence"],"snapshot_digest":claim["snapshot_digest"],"observed_account_id":self.sender,"observed_group_id":self.group,"client_version":"client1","script_version":"script1","baseline_version":"base1","checks_passed":true})).await;
        (claim, prep)
    }
    fn permit_input(claim: &Value, prep: &Value) -> Command {
        command(
            json!({"operation":"permit","fence":claim["fence"],"snapshot_digest":claim["snapshot_digest"],"preparation_id":prep["id"]}),
        )
    }
}
async fn setup() -> Fixture {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("mobile_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {schema},public");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let actor = Uuid::new_v4();
    let school = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'admin','Admin',true)").bind(actor).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'School')")
        .bind(school)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO school_memberships VALUES($1,$2,'admin')")
        .bind(school)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    let repo = Repository(pool.clone());
    async fn call(
        repo: &Repository,
        actor: Uuid,
        s: Option<Uuid>,
        id: Option<Uuid>,
        v: Value,
    ) -> Value {
        repo.execute(
            Principal::Actor(actor),
            Scope { school: s, id },
            &Uuid::new_v4().to_string(),
            command(v),
            true,
            "https://example.test",
        )
        .await
        .unwrap()
    }
    let sender=uuid(&call(&repo,actor,None,None,json!({"operation":"create_account","client_kind":"wechat","identity_namespace":"verified-tenant","external_key":"real-account","responsible_actor_id":actor,"identity_evidence_ref":"identity:1"})).await,"id");
    let group=uuid(&call(&repo,actor,Some(school),None,json!({"operation":"create_group","name":"Group","client_kind":"wechat","identity_namespace":"verified-tenant","external_key":"real-group","identity_evidence_ref":"group:1","wecom_group_id":null})).await,"id");
    call(&repo,actor,Some(school),Some(group),json!({"operation":"bind_group","sender_account_id":sender,"mapping_revision":1,"reason":"verified identity"})).await;
    let device = uuid(
        &call(
            &repo,
            actor,
            None,
            None,
            json!({"operation":"create_device","name":"Phone"}),
        )
        .await,
        "id",
    );
    call(&repo,actor,None,Some(device),json!({"operation":"bind_device","expected_revision":1,"sender_account_id":sender,"identity_evidence_ref":"device:1"})).await;
    let enrollment = call(
        &repo,
        actor,
        None,
        Some(device),
        json!({"operation":"enrollment","expected_revision":2}),
    )
    .await;
    let cred=repo.execute(Principal::Enrollment,Scope{school:None,id:None},"pair",command(json!({"operation":"enroll","device_id":device,"code":enrollment["code"],"client_instance_id":"phone1","protocol_version":1})),true,"https://example.test").await.unwrap();
    call(&repo,actor,None,Some(device),json!({"operation":"update_device","expected_revision":3,"enabled":true,"reason":"accepted executor"})).await;
    call(&repo,actor,Some(school),None,json!({"operation":"settings","expected_revision":0,"enabled":true,"history_reviewed":true,"approved_client_version":"client1","approved_script_version":"script1","approved_baseline_version":"base1","evidence_ref":"acceptance:1"})).await;
    let token = cred["device_token"].as_str().unwrap().to_owned();
    let principal = repo.authenticate(&token).await.unwrap();
    Fixture {
        pool,
        repo,
        actor,
        school,
        sender,
        group,
        device,
        principal,
        binding_revision: 2,
        token,
    }
}
#[tokio::test]
#[ignore]
async fn complete_protocol_does_not_invent_official_success_and_blocks_repeats() {
    let f = setup().await;
    let (c, p) = f.prepared(" Café\r\n学校通知 ").await;
    let permit = f
        .repo
        .execute(
            f.principal.clone(),
            Scope {
                school: None,
                id: Some(uuid(&c, "id")),
            },
            "permit",
            Fixture::permit_input(&c, &p),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let i=f.device_call(Some(uuid(&c,"id")),json!({"operation":"result","invocation_id":permit["invocation_id"],"permit_token":permit["permit_token"],"event_id":Uuid::new_v4(),"result":"ui_observed_sent","evidence_ref":"ui:1","observed_account_id":f.sender,"observed_group_id":f.group,"content_digest":c["content_digest"],"local_state":"sent_bubble_observed","reported_clicked_at":chrono::Utc::now()})).await;
    assert_eq!(i["state"], "ui_observed_sent");
    assert!(i.get("official_status").is_none());
    let result = f
        .repo
        .execute(
            f.principal.clone(),
            Scope {
                school: None,
                id: Some(uuid(&c, "id")),
            },
            "new-permit",
            Fixture::permit_input(&c, &p),
            true,
            "https://example.test",
        )
        .await;
    assert!(result.is_err());
    let t = f.task("Café\n学校通知").await;
    let preview = f
        .call(
            Some(f.school),
            Some(uuid(&t, "id")),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await;
    assert_eq!(preview["items"][0]["reason"], "duplicate_delivery");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_send_permits")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
#[ignore]
async fn lost_permit_response_keeps_one_invocation_and_unknown_hold() {
    let f = setup().await;
    let (c, p) = f.prepared("独立通知").await;
    let scope = Scope {
        school: None,
        id: Some(uuid(&c, "id")),
    };
    let cmd = Fixture::permit_input(&c, &p);
    let first = f
        .repo
        .execute(
            f.principal.clone(),
            scope.clone(),
            "same-key",
            cmd.clone(),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let replay = f
        .repo
        .execute(
            f.principal.clone(),
            scope.clone(),
            "same-key",
            cmd,
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert_eq!(first["invocation_id"], replay["invocation_id"]);
    assert!(replay.get("permit_token").is_none());
    assert_eq!(replay["reason"], "unknown_requires_review");
    let state: String =
        sqlx::query_scalar("SELECT state FROM delivery_usage WHERE source_channel='mobile'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(state, "unknown");
    f.repo.recover().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_send_permits")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let wait = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(wait["waiting"], true);
}
#[tokio::test]
#[ignore]
async fn concurrent_permit_requests_cannot_double_reserve() {
    let f = setup().await;
    let (c, p) = f.prepared("并发发送测试").await;
    let scope = Scope {
        school: None,
        id: Some(uuid(&c, "id")),
    };
    let cmd = Fixture::permit_input(&c, &p);
    let (a, b) = tokio::join!(
        f.repo.execute(
            f.principal.clone(),
            scope.clone(),
            "a",
            cmd.clone(),
            true,
            "https://example.test"
        ),
        f.repo.execute(
            f.principal.clone(),
            scope,
            "b",
            cmd,
            true,
            "https://example.test"
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM delivery_usage")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore]
async fn old_fence_revocation_and_global_disable_fail_closed() {
    let f = setup().await;
    let (c, p) = f.prepared("安全发送测试").await;
    let scope = Scope {
        school: None,
        id: Some(uuid(&c, "id")),
    };
    assert!(matches!(
        f.repo
            .execute(
                f.principal.clone(),
                scope.clone(),
                "disabled",
                Fixture::permit_input(&c, &p),
                false,
                "https://example.test"
            )
            .await,
        Err(Error::Unavailable)
    ));
    sqlx::query("UPDATE mobile_execution_claims SET lease_until=now()-interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    f.repo.recover().await.unwrap();
    let c2 = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(c2["fence"].as_i64().unwrap() > c["fence"].as_i64().unwrap());
    assert!(
        f.repo
            .execute(
                f.principal.clone(),
                scope,
                "stale",
                Fixture::permit_input(&c, &p),
                true,
                "https://example.test"
            )
            .await
            .is_err()
    );
    f.call(
        None,
        Some(f.device),
        json!({"operation":"revoke","expected_revision":4,"reason":"device lost"}),
    )
    .await;
    assert!(f.repo.authenticate(&f.token).await.is_err());
}
#[tokio::test]
#[ignore]
async fn platform_admin_without_school_membership_cannot_access_business() {
    let f = setup().await;
    let actor = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'outsider','Outsider',true)").bind(actor).execute(&f.pool).await.unwrap();
    assert!(matches!(
        f.repo
            .query(
                Principal::Actor(actor),
                Scope {
                    school: Some(f.school),
                    id: None
                },
                Query::Tasks,
                Page::default()
            )
            .await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
#[ignore]
async fn quota_cooldown_boundaries_and_unknown_never_age_out() {
    let f = setup().await;
    let t = f.task("额度边界通知").await;
    async fn check(f: &Fixture, t: &Value) -> Value {
        f.call(
            Some(f.school),
            Some(uuid(t, "id")),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await["items"][0]["reason"]
            .clone()
    }
    sqlx::query("INSERT INTO delivery_usage(id,source_channel,source_id,school_id,group_id,sender_account_id,content_digest,state,observed_at,evidence_kind) SELECT gen_random_uuid(),'mobile',gen_random_uuid(),$1,$2,$3,'other-'||i::text,'sent',now()-interval '31 minutes','ui' FROM generate_series(1,30) i").bind(f.school).bind(f.group).bind(f.sender).execute(&f.pool).await.unwrap();
    assert_eq!(check(&f, &t).await, "quota_exhausted");
    f.call(None,Some(f.sender),json!({"operation":"account_policy","expected_revision":1,"daily_limit":50,"reason":"approved limit"})).await;
    // Account increase cannot bypass the group's default 10.
    assert_eq!(check(&f, &t).await, "quota_exhausted");
    f.call(Some(f.school),Some(f.group),json!({"operation":"group_policy","expected_revision":2,"daily_limit":30,"reason":"approved limit"})).await;
    assert_eq!(check(&f, &t).await, "quota_exhausted");
    sqlx::query("DELETE FROM delivery_usage WHERE id IN (SELECT id FROM delivery_usage LIMIT 1)")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(check(&f, &t).await, Value::Null);
    sqlx::query("UPDATE delivery_usage SET observed_at=now()-interval '29 minutes 59 seconds' WHERE id=(SELECT id FROM delivery_usage LIMIT 1)").execute(&f.pool).await.unwrap();
    assert_eq!(check(&f, &t).await, "cooldown_active");
    sqlx::query("UPDATE delivery_usage SET observed_at=now()-interval '30 minutes'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(check(&f, &t).await, Value::Null);
    sqlx::query("UPDATE delivery_usage SET state='unknown',observed_at=now()-interval '100 days' WHERE id=(SELECT id FROM delivery_usage LIMIT 1)").execute(&f.pool).await.unwrap();
    assert_eq!(check(&f, &t).await, "unknown_requires_review");
    f.call(None,Some(f.sender),json!({"operation":"account_policy","expected_revision":2,"daily_limit":0,"reason":"pause"})).await;
    assert_eq!(check(&f, &t).await, "quota_exhausted");
}
#[tokio::test]
#[ignore]
async fn reconciliation_waits_out_remote_window_then_requires_new_confirmed_draft() {
    let f = setup().await;
    let (c, p) = f.prepared("需核对通知").await;
    let _permit = f
        .repo
        .execute(
            f.principal.clone(),
            Scope {
                school: None,
                id: Some(uuid(&c, "id")),
            },
            "permit",
            Fixture::permit_input(&c, &p),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let i = f
        .device_call(
            Some(uuid(&c, "id")),
            json!({"operation":"abort","fence":c["fence"],"reason":"lost local observation"}),
        )
        .await;
    assert_eq!(i["state"], "unknown");
    let cmd = command(
        json!({"operation":"reconcile","expected_revision":i["revision"],"decision":"verified_not_sent","evidence_ref":"verified:1","reason":"manual check"}),
    );
    assert!(matches!(
        f.repo
            .execute(
                Principal::Actor(f.actor),
                Scope {
                    school: Some(f.school),
                    id: Some(uuid(&i, "id"))
                },
                "early-review",
                cmd.clone(),
                true,
                "https://example.test"
            )
            .await,
        Err(Error::Policy("permit_expired"))
    ));
    sqlx::query("UPDATE mobile_send_permits SET execute_before=now()-interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    let reviewed = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            Scope {
                school: Some(f.school),
                id: Some(uuid(&i, "id")),
            },
            "review",
            cmd,
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let draft=f.call(Some(f.school),Some(uuid(&i,"id")),json!({"operation":"resend","expected_revision":reviewed["revision"],"reason":"approved new attempt"})).await;
    assert_eq!(draft["state"], "draft");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_send_permits")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM delivery_usage")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "not_sent"
    );
}
#[tokio::test]
#[ignore]
async fn actual_http_device_auth_contract_and_management_csrf_are_separate() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let f = setup().await;
    let mut app = wekeeper_api::bootstrap::build_application(f.pool.clone());
    app.options.public_origin = "https://example.test".into();
    let (session, actor) = app
        .identity
        .0
        .session(f.actor, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/device-execution/session")
                .header("Authorization", format!("Bearer {}", f.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let doc: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(doc["data"]["type"], "device-execution-sessions");
    assert_eq!(doc["data"]["id"], f.device.to_string());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/schools/{}/mobile-delivery-tasks",
                    f.school
                ))
                .header("Authorization", format!("Bearer {}", f.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let request = |csrf: bool| {
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/v1/platform/execution-devices")
            .header("Cookie", format!("wekeeper_session={session}"))
            .header("Origin", "https://example.test")
            .header("Content-Type", "application/vnd.api+json")
            .header("Idempotency-Key", "http-create");
        if csrf {
            req = req.header("X-CSRF-Token", &actor.csrf);
        }
        req.body(Body::from(
            json!({"data":{"type":"execution-devices","attributes":{"name":"HTTP phone"}}})
                .to_string(),
        ))
        .unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request(false))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = router.oneshot(request(true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let doc: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(doc["data"]["attributes"]["enabled"], true);
    assert!(doc["data"]["attributes"].get("device_token").is_none());
}

#[tokio::test]
#[ignore]
async fn identical_content_cooldown_is_global_across_schools_and_accounts() {
    let f = setup().await;
    let t = f.task("跨学校相同正文").await;
    let other = Uuid::new_v4();
    let group = Uuid::new_v4();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'Other')")
        .bind(other)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO delivery_groups(id,school_id,name,client_kind,identity_namespace,external_key,identity_evidence_ref) VALUES($1,$2,'Other','wechat','other','other','verified')").bind(group).bind(other).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO delivery_usage(id,source_channel,source_id,school_id,group_id,content_digest,state,observed_at,evidence_kind) VALUES(gen_random_uuid(),'mobile',gen_random_uuid(),$1,$2,$3,'sent',now()-interval '9 minutes 59 seconds','ui')").bind(other).bind(group).bind(digest("跨学校相同正文").unwrap()).execute(&f.pool).await.unwrap();
    let preview = f
        .call(
            Some(f.school),
            Some(uuid(&t, "id")),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await;
    assert_eq!(preview["items"][0]["reason"], "cooldown_active");
    sqlx::query("UPDATE delivery_usage SET observed_at=now()-interval '10 minutes'")
        .execute(&f.pool)
        .await
        .unwrap();
    let preview = f
        .call(
            Some(f.school),
            Some(uuid(&t, "id")),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await;
    assert_eq!(preview["items"][0]["reason"], Value::Null);
    sqlx::query("UPDATE delivery_usage SET state='unknown',observed_at=now()-interval '7 days'")
        .execute(&f.pool)
        .await
        .unwrap();
    let preview = f
        .call(
            Some(f.school),
            Some(uuid(&t, "id")),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await;
    assert_eq!(preview["items"][0]["reason"], "unknown_requires_review");
}
#[tokio::test]
#[ignore]
async fn group_round_robin_advances_after_unpermitted_abort() {
    let f = setup().await;
    let g=f.call(Some(f.school),None,json!({"operation":"create_group","name":"Second","client_kind":"wechat","identity_namespace":"verified-tenant","external_key":"second","identity_evidence_ref":"group:2","wecom_group_id":null})).await;
    let group = uuid(&g, "id");
    f.call(Some(f.school),Some(group),json!({"operation":"bind_group","sender_account_id":f.sender,"mapping_revision":1,"reason":"verified second group"})).await;
    let t=f.call(Some(f.school),None,json!({"operation":"draft","activity_id":"notices","items":[{"group_id":f.group,"sender_account_id":f.sender,"source":{"kind":"text","text_content":"第一群独立通知"}},{"group_id":group,"sender_account_id":f.sender,"source":{"kind":"text","text_content":"第二群不同事项"}}]})).await;
    f.confirm(&t).await;
    let c1 = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    f.device_call(
        Some(uuid(&c1, "id")),
        json!({"operation":"abort","fence":c1["fence"],"reason":"not ready"}),
    )
    .await;
    let c2 = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_ne!(c1["group_snapshot"]["id"], c2["group_snapshot"]["id"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM delivery_usage")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
#[ignore]
async fn official_history_projection_is_idempotent_and_mobile_switch_blocks_old_writes() {
    use wekeeper_api::distribution::south::adapter::postgres::mobile_delivery::{
        guard, official_allowed, project,
    };
    let f = setup().await;
    let official_group = Uuid::new_v4();
    let group = Uuid::new_v4();
    let task = Uuid::new_v4();
    let batch = Uuid::new_v4();
    let target = Uuid::new_v4();
    sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name) VALUES($1,$2,'corp','chat','Official')").bind(official_group).bind(f.school).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO delivery_groups(id,school_id,name,client_kind,identity_namespace,external_key,identity_evidence_ref,wecom_group_id) VALUES($1,$2,'Official','wecom','corp','chat','verified',$3)").bind(group).bind(f.school).bind(official_group).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_tasks(id,school_id,creator_id,content,content_hash,corp_id,generation,content_source,state,confirmed_by) VALUES($1,$2,$3,' Café ','old-hash-unchanged','corp',1,'text','confirmed',$3)").bind(task).bind(f.school).bind(f.actor).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_batches(id,school_id,task_id,ordinal,sender,sender_account,binding_revision,state,corp_id,msgid) VALUES($1,$2,$3,1,'sender',$4,1,'created','corp','msgid')").bind(batch).bind(f.school).bind(task).bind(f.actor).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_targets(id,school_id,task_id,batch_id,group_id,chat_id,group_name,group_revision,official_status,send_time) VALUES($1,$2,$3,$4,$5,'chat','Official',1,1,extract(epoch from now())::bigint)").bind(target).bind(f.school).bind(task).bind(batch).bind(official_group).execute(&f.pool).await.unwrap();
    for _ in 0..2 {
        let mut tx = f.pool.begin().await.unwrap();
        guard(&mut tx).await.unwrap();
        project(&mut tx).await.unwrap();
        assert!(matches!(
            official_allowed(&mut tx, f.school).await,
            Err(Error::Policy("mobile_channel_exclusive"))
        ));
        tx.commit().await.unwrap();
    }
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT source_channel,state,content_digest FROM delivery_usage")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![("official".into(), "sent".into(), digest("Café").unwrap())]
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT content_hash FROM broadcast_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "old-hash-unchanged"
    );
    // A stopped official batch without complete per-target evidence remains unknown.
    sqlx::query("UPDATE broadcast_targets SET official_status=NULL WHERE id=$1")
        .bind(target)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE broadcast_batches SET stop_state='stopped',reconciliation='not_created' WHERE id=$1")
        .bind(batch)
        .execute(&f.pool)
        .await
        .unwrap();
    f.repo.recover().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM delivery_usage")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "unknown"
    );
}

#[tokio::test]
#[ignore]
async fn post_snapshots_recheck_revision_and_render_existing_community_urls() {
    let f = setup().await;
    let template = Uuid::new_v4();
    let version = Uuid::new_v4();
    let post = Uuid::new_v4();
    sqlx::query("INSERT INTO templates(id,school_id,name) VALUES($1,$2,'Notice')")
        .bind(template)
        .bind(f.school)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO template_versions(id,school_id,template_id,body,published) VALUES($1,$2,$3,'{{post.title}} {{post.detail_url}}',true)").bind(version).bind(f.school).bind(template).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO posts(id,school_id,author_id,title,content,audit_status) VALUES($1,$2,$3,'Notice','Approved body','approved')").bind(post).bind(f.school).bind(f.actor).execute(&f.pool).await.unwrap();
    let t=f.call(Some(f.school),None,json!({"operation":"draft","activity_id":"approved-post","items":[{"group_id":f.group,"sender_account_id":f.sender,"source":{"kind":"posts","template_version_id":version,"post_ids":[post]}}]})).await;
    f.confirm(&t).await;
    let c = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(c["content"].as_str().unwrap().contains(&format!(
        "https://example.test/community/{}/posts/{post}",
        f.school
    )));
    let prep=f.device_call(Some(uuid(&c,"id")),json!({"operation":"prepare","fence":c["fence"],"snapshot_digest":c["snapshot_digest"],"observed_account_id":f.sender,"observed_group_id":f.group,"client_version":"client1","script_version":"script1","baseline_version":"base1","checks_passed":true})).await;
    sqlx::query("UPDATE posts SET revision=revision+1 WHERE id=$1")
        .bind(post)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.repo
            .execute(
                f.principal.clone(),
                Scope {
                    school: None,
                    id: Some(uuid(&c, "id"))
                },
                "permit",
                Fixture::permit_input(&c, &prep),
                true,
                "https://example.test"
            )
            .await,
        Err(Error::Policy("binding_changed"))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_send_permits")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
#[ignore]
async fn two_accounts_preparing_same_group_still_get_only_one_permit() {
    let f = setup().await;
    let (c1, p1) = f.prepared("账号一的独立事项").await;
    let second=f.call(None,None,json!({"operation":"create_account","client_kind":"wechat","identity_namespace":"verified-tenant","external_key":"second-account","responsible_actor_id":f.actor,"identity_evidence_ref":"identity:second"})).await;
    let sender = uuid(&second, "id");
    f.call(Some(f.school),Some(f.group),json!({"operation":"bind_group","sender_account_id":sender,"mapping_revision":2,"reason":"same actual group verified"})).await;
    let d = Uuid::new_v4();
    sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Second phone',true,1)").bind(d).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES('test-only-hash',$1,1,now()+interval '1 day')").bind(d).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO device_account_bindings(device_id,sender_account_id,revision,identity_evidence_ref) VALUES($1,$2,1,'verified')").bind(d).bind(sender).execute(&f.pool).await.unwrap();
    let device = Principal::Device {
        id: d,
        generation: 1,
    };
    let task=f.call(Some(f.school),None,json!({"operation":"draft","activity_id":"second-notice","items":[{"group_id":f.group,"sender_account_id":sender,"source":{"kind":"text","text_content":"账号二的不同事项"}}]})).await;
    f.confirm(&task).await;
    let c2 = f
        .repo
        .execute(
            device.clone(),
            Scope {
                school: None,
                id: None,
            },
            "claim-2",
            command(json!({"operation":"claim","binding_revision":1})),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let p2=f.repo.execute(device.clone(),Scope{school:None,id:Some(uuid(&c2,"id"))},"prepare-2",command(json!({"operation":"prepare","fence":c2["fence"],"snapshot_digest":c2["snapshot_digest"],"observed_account_id":sender,"observed_group_id":f.group,"client_version":"client1","script_version":"script1","baseline_version":"base1","checks_passed":true})),true,"https://example.test").await.unwrap();
    let (a, b) = tokio::join!(
        f.repo.execute(
            f.principal.clone(),
            Scope {
                school: None,
                id: Some(uuid(&c1, "id"))
            },
            "permit-1",
            Fixture::permit_input(&c1, &p1),
            true,
            "https://example.test"
        ),
        f.repo.execute(
            device,
            Scope {
                school: None,
                id: Some(uuid(&c2, "id"))
            },
            "permit-2",
            Fixture::permit_input(&c2, &p2),
            true,
            "https://example.test"
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_send_permits")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
#[ignore]
async fn absent_click_timestamp_cannot_create_success_or_release_unknown_hold() {
    for reported_result in ["not_observed", "ui_observed_sent"] {
        let f = setup().await;
        let (claim, preparation) = f.prepared("点击前停止，结果保守核对").await;
        let scope = Scope {
            school: None,
            id: Some(uuid(&claim, "id")),
        };
        let permit = f
            .repo
            .execute(
                f.principal.clone(),
                scope.clone(),
                "permit-null-click",
                Fixture::permit_input(&claim, &preparation),
                true,
                "https://example.test",
            )
            .await
            .unwrap();
        let input = json!({
            "operation": "result", "invocation_id": permit["invocation_id"],
            "permit_token": permit["permit_token"], "event_id": Uuid::new_v4(),
            "result": reported_result, "evidence_ref": "otter-local:test/no-click",
            "observed_account_id": f.sender, "observed_group_id": f.group,
            "content_digest": claim["content_digest"], "local_state": "sent_bubble_observed",
            "reported_clicked_at": null
        });
        let first = f
            .repo
            .execute(
                f.principal.clone(),
                scope.clone(),
                "null-click-result",
                command(input.clone()),
                true,
                "https://example.test",
            )
            .await
            .unwrap();
        let replay = f
            .repo
            .execute(
                f.principal.clone(),
                scope,
                "null-click-result",
                command(input),
                true,
                "https://example.test",
            )
            .await
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(first["state"], "unknown");
        let states: Vec<String> =
            sqlx::query_scalar("SELECT state FROM delivery_usage WHERE source_channel='mobile'")
                .fetch_all(&f.pool)
                .await
                .unwrap();
        assert_eq!(states, vec!["unknown"]);
        let next = f
            .repo
            .execute(
                f.principal.clone(),
                Scope {
                    school: None,
                    id: Some(uuid(&claim, "id")),
                },
                "cannot-repermit-null-click",
                Fixture::permit_input(&claim, &preparation),
                true,
                "https://example.test",
            )
            .await;
        assert!(next.is_err());
    }
}

// All tests use a fresh schema in the explicitly isolated test database.
async fn visible_fixture() -> Fixture {
    let mut f = setup().await;
    let a = f.call(None,None,json!({"operation":"create_account","client_kind":"wecom","identity_mode":"ui_visible","visible_profile":{"company_name":"测试企业","account_name":"测试账号"},"responsible_actor_id":f.actor,"identity_evidence_ref":"phone:account"})).await;
    let g = f.call(Some(f.school),None,json!({"operation":"create_group","client_kind":"wecom","identity_mode":"ui_visible","name":"机器人测试","visible_profile":{"group_type":"external"},"identity_evidence_ref":"phone:group"})).await;
    assert!(a["external_key"].is_null() && g["identity_namespace"].is_null());
    f.sender = uuid(&a, "id");
    f.group = uuid(&g, "id");
    f.call(None,Some(f.sender),json!({"operation":"review_account_profile","expected_revision":1,"evidence_ref":"phone:review-account"})).await;
    f.call(Some(f.school),Some(f.group),json!({"operation":"review_group_profile","expected_revision":1,"evidence_ref":"phone:review-group"})).await;
    f.call(Some(f.school),Some(f.group),json!({"operation":"bind_group","sender_account_id":f.sender,"mapping_revision":2,"reason":"visible profile checked"})).await;
    let rev: i64 = sqlx::query_scalar("SELECT revision FROM execution_devices WHERE id=$1")
        .bind(f.device)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    f.call(None,Some(f.device),json!({"operation":"bind_device","expected_revision":rev,"sender_account_id":f.sender,"identity_evidence_ref":"phone:bind"})).await;
    f.binding_revision =
        sqlx::query_scalar("SELECT revision FROM device_account_bindings WHERE device_id=$1")
            .bind(f.device)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    f.principal = f.principal.for_protocol(2);
    f.device_call(None,json!({"operation":"heartbeat","binding_revision":f.binding_revision,"sender_account_id":f.sender,"client_version":"client1","script_version":"script1","baseline_version":"base1","permissions_ready":true,"current_claim_id":null})).await;
    f
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn responsible_actors_filter_search_page_and_recheck_registration() {
    let f = setup().await;
    let inactive_school = Uuid::new_v4();
    let other_school = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO schools(id,name,active) VALUES($1,'Inactive',false),($2,'Other',true)",
    )
    .bind(inactive_school)
    .bind(other_school)
    .execute(&f.pool)
    .await
    .unwrap();
    let mut users = Vec::new();
    for (name, active, platform_admin, school, role) in [
        ("candidate_one", true, false, f.school, "admin"),
        ("candidate_two", true, false, other_school, "admin"),
        ("candidate_disabled", false, false, f.school, "admin"),
        ("candidate_operator", true, false, f.school, "operator"),
        ("candidate_revoked", true, false, f.school, "revoked"),
        (
            "candidate_inactive_school",
            true,
            false,
            inactive_school,
            "admin",
        ),
        ("candidate_platform_only", true, true, f.school, "member"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts(id,username,display_name,active,platform_admin) VALUES($1,$2,'同名责任人',$3,$4)")
            .bind(id).bind(name).bind(active).bind(platform_admin).execute(&f.pool).await.unwrap();
        sqlx::query("INSERT INTO school_memberships VALUES($1,$2,$3)")
            .bind(school)
            .bind(id)
            .bind(role)
            .execute(&f.pool)
            .await
            .unwrap();
        users.push(id);
    }
    let scope = Scope {
        school: None,
        id: None,
    };
    let candidates = f
        .repo
        .query(
            Principal::Actor(f.actor),
            scope.clone(),
            Query::ResponsibleActors,
            Page {
                search: Some("CaNdIdAtE".into()),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Store fetches a sentinel row; HTTP collection uses it for has_more.
    assert_eq!(candidates.as_array().unwrap().len(), 2);
    let first: ResponsibleActor = serde_json::from_value(candidates[0].clone()).unwrap();
    let second: ResponsibleActor = serde_json::from_value(candidates[1].clone()).unwrap();
    assert_eq!(first.id, users[0]);
    assert_eq!(second.id, users[1]);
    assert!(first.active && first.eligible);
    assert_eq!(second.schools[0].id, other_school);
    let page_two = f
        .repo
        .query(
            Principal::Actor(f.actor),
            scope.clone(),
            Query::ResponsibleActors,
            Page {
                search: Some("candidate".into()),
                page: Some(2),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page_two, json!([second]));
    for search in ["%", "_", "不存在"] {
        let found = f
            .repo
            .query(
                Principal::Actor(f.actor),
                scope.clone(),
                Query::ResponsibleActors,
                Page {
                    search: Some(search.into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // '_' is literal: every candidate login contains it. '%' is not a wildcard.
        assert_eq!(
            found.as_array().unwrap().len(),
            if search == "_" { 2 } else { 0 }
        );
    }
    let by_name = f
        .repo
        .query(
            Principal::Actor(f.actor),
            scope.clone(),
            Query::ResponsibleActors,
            Page {
                search: Some("同名责任人".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(by_name.as_array().unwrap().len(), 2);
    assert!(matches!(
        f.repo
            .query(
                Principal::Actor(users[0]),
                scope.clone(),
                Query::ResponsibleActors,
                Page::default()
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(
        f.repo
            .query(
                Principal::Actor(f.actor),
                scope.clone(),
                Query::ResponsibleActors,
                Page {
                    search: Some("中".repeat(101)),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );

    sqlx::query("UPDATE school_memberships SET role='revoked' WHERE account_id=$1")
        .bind(users[0])
        .execute(&f.pool)
        .await
        .unwrap();
    for actor in [
        users[0],
        users[2],
        users[3],
        users[4],
        users[5],
        users[6],
        Uuid::new_v4(),
    ] {
        for visible in [false, true] {
            let input = if visible {
                json!({"operation":"create_account","client_kind":"wecom","identity_mode":"ui_visible","visible_profile":{"company_name":"测试企业","account_name":"不可用"},"responsible_actor_id":actor,"identity_evidence_ref":"phone:account"})
            } else {
                json!({"operation":"create_account","client_kind":"wechat","identity_namespace":"test","external_key":"not-eligible","responsible_actor_id":actor,"identity_evidence_ref":"phone:account"})
            };
            let result = f
                .repo
                .execute(
                    Principal::Actor(f.actor),
                    scope.clone(),
                    &Uuid::new_v4().to_string(),
                    command(input),
                    true,
                    "https://example.test",
                )
                .await;
            assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
        }
    }
    // A valid B-school administrator is registrable at platform level, but cannot
    // authorize this account for A-school groups merely because the creator is platform admin.
    let created = f.call(None, None, json!({"operation":"create_account","client_kind":"wecom","identity_mode":"ui_visible","visible_profile":{"company_name":"测试企业","account_name":"Other school account"},"responsible_actor_id":users[1],"identity_evidence_ref":"B校手机账号登记"})).await;
    assert_eq!(created["verification_state"], "pending");
    let group = f.call(Some(f.school), None, json!({"operation":"create_group","name":"A校群","client_kind":"wecom","identity_mode":"ui_visible","visible_profile":{"group_type":"external"},"identity_evidence_ref":"A校群登记"})).await;
    let result = f.repo.execute(Principal::Actor(f.actor), Scope { school: Some(f.school), id: Some(uuid(&group,"id")) }, "cross-school", command(json!({"operation":"bind_group","sender_account_id":created["id"],"mapping_revision":1,"reason":"cross-school denied"})), true, "https://example.test").await;
    assert!(matches!(result, Err(Error::Forbidden)));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn account_projection_keeps_registration_review_actor_and_stale_history() {
    let f = setup().await;
    let responsible = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name) VALUES($1,'responsible','责任人')")
        .bind(responsible)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO school_memberships VALUES($1,$2,'admin')")
        .bind(f.school)
        .bind(responsible)
        .execute(&f.pool)
        .await
        .unwrap();
    let input = command(
        json!({"operation":"create_account","client_kind":"wecom","identity_mode":"ui_visible","visible_profile":{"company_name":"测试企业","account_name":"实际账号"},"responsible_actor_id":responsible,"identity_evidence_ref":"登记时手机账号页面"}),
    );
    let scope = Scope {
        school: None,
        id: None,
    };
    let created = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "create-stable",
            input.clone(),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert_eq!(
        created,
        f.repo
            .execute(
                Principal::Actor(f.actor),
                scope,
                "create-stable",
                input,
                true,
                "https://example.test"
            )
            .await
            .unwrap()
    );
    let scope = Scope {
        school: None,
        id: Some(uuid(&created, "id")),
    };
    for evidence in [" ".into(), "字".repeat(501)] {
        assert!(f.repo.execute(Principal::Actor(f.actor), scope.clone(), &Uuid::new_v4().to_string(), command(json!({"operation":"review_account_profile","expected_revision":1,"evidence_ref":evidence})), true, "https://example.test").await.is_err());
    }
    let review = command(
        json!({"operation":"review_account_profile","expected_revision":1,"evidence_ref":"本次现场核对企业及账号资料"}),
    );
    let reviewed = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "review-stable",
            review.clone(),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert_eq!(
        reviewed,
        f.repo
            .execute(
                Principal::Actor(f.actor),
                scope.clone(),
                "review-stable",
                review,
                true,
                "https://example.test"
            )
            .await
            .unwrap()
    );
    let projection = f
        .repo
        .query(
            Principal::Actor(f.actor),
            scope.clone(),
            Query::AccountPolicy,
            Page::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        projection["registration_evidence_ref"],
        "登记时手机账号页面"
    );
    assert_eq!(
        projection["latest_profile_review"]["actor_id"],
        json!(f.actor)
    );
    assert_ne!(
        projection["latest_profile_review"]["actor_id"],
        json!(responsible)
    );
    assert_eq!(projection["latest_profile_review"]["profile_revision"], 1);
    assert!(projection["latest_profile_review"]["reviewed_at"].is_string());
    assert_eq!(projection["responsible_actor"]["username"], "responsible");
    f.call(None, scope.id, json!({"operation":"update_account_profile","expected_revision":2,"visible_profile":{"company_name":"测试企业","account_name":"更新账号"},"reason":"实际资料变化"})).await;
    let stale = f.repo.execute(Principal::Actor(f.actor), scope.clone(), "stale-review", command(json!({"operation":"review_account_profile","expected_revision":2,"evidence_ref":"旧页提交"})), true, "https://example.test").await;
    assert!(matches!(stale, Err(Error::Policy("revision_conflict"))));
    sqlx::query("UPDATE accounts SET active=false WHERE id=$1")
        .bind(responsible)
        .execute(&f.pool)
        .await
        .unwrap();
    let projection = f
        .repo
        .query(
            Principal::Actor(f.actor),
            scope.clone(),
            Query::AccountPolicy,
            Page::default(),
        )
        .await
        .unwrap();
    assert_eq!(projection["verification_state"], "pending");
    assert_eq!(projection["profile_revision"], 2);
    assert_eq!(projection["responsible_actor"]["eligible"], false);
    assert_eq!(projection["responsible_actor"]["display_name"], "责任人");
    assert_eq!(projection["latest_profile_review"]["profile_revision"], 1);
    assert_eq!(
        projection["registration_evidence_ref"],
        "登记时手机账号页面"
    );
    let list = f
        .repo
        .query(
            Principal::Actor(f.actor),
            Scope {
                school: None,
                id: None,
            },
            Query::Accounts,
            Page::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == created["id"])
            .unwrap(),
        &projection
    );
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_delivery_events WHERE action='review_account_profile' AND detail->>'scope_id'=$1").bind(scope.id.unwrap().to_string()).fetch_one(&f.pool).await.unwrap();
    assert_eq!(events, 1);
}
async fn visible_run(f: &Fixture) -> (Value, Value) {
    let strategy=f.call(Some(f.school),None,json!({"operation":"create_strategy","name":"指定群测试","sender_account_id":f.sender,"group_id":f.group,"device_id":f.device,"text_content":"测试","trigger_mode":"manual_once","enabled":true})).await;
    let scope = Scope {
        school: Some(f.school),
        id: Some(uuid(&strategy, "id")),
    };
    let input = command(json!({"operation":"run_strategy","expected_revision":1}));
    let key = Uuid::new_v4().to_string();
    let run = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            &key,
            input.clone(),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let replay = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            &key,
            input,
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert_eq!(run, replay);
    assert!(
        f.repo
            .execute(
                Principal::Actor(f.actor),
                scope,
                &key,
                command(json!({"operation":"run_strategy","expected_revision":2})),
                true,
                "https://example.test"
            )
            .await
            .is_err()
    );
    let task = f
        .repo
        .query(
            Principal::Actor(f.actor),
            Scope {
                school: Some(f.school),
                id: Some(uuid(&run, "task_id")),
            },
            Query::Task,
            Page::default(),
        )
        .await
        .unwrap();
    assert_eq!(task["state"], "draft");
    (run, task)
}
#[tokio::test]
#[ignore]
async fn visible_strategy_v2_is_draft_idempotent_and_requires_verification() {
    let f = visible_fixture().await;
    let (_, task) = visible_run(&f).await;
    let wait = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(wait["claim_id"].is_null());
    f.confirm(&task).await;
    let legacy = f
        .repo
        .execute(
            f.principal.clone().for_protocol(1),
            Scope {
                school: None,
                id: None,
            },
            "legacy-cannot-claim-ui",
            command(json!({"operation":"claim","binding_revision":f.binding_revision})),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert!(legacy["claim_id"].is_null());
    let claim = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(claim["requirements"]["protocol_version"], 2);
    assert_eq!(claim["execution_target"]["device_id"], f.device.to_string());
    assert_eq!(claim["group_snapshot"]["name"], "机器人测试");
    for key in [
        "external_key",
        "identity_namespace",
        "wecom_group_id",
        "resource_id",
    ] {
        assert!(claim["sender_binding"].get(key).is_none());
        assert!(claim["group_snapshot"].get(key).is_none());
    }
    let scope = Scope {
        school: None,
        id: Some(uuid(&claim, "id")),
    };
    let mut prepare = json!({"operation":"prepare","fence":claim["fence"],"snapshot_digest":claim["snapshot_digest"],"observed_account_id":f.sender,"observed_group_id":f.group,"client_version":"client1","script_version":"script1","baseline_version":"base1","checks_passed":true});
    assert!(
        f.repo
            .execute(
                f.principal.clone(),
                scope.clone(),
                "missing-verification",
                command(prepare.clone()),
                true,
                "https://example.test"
            )
            .await
            .is_err()
    );
    prepare["verification"] = json!({"method":"visible_ui","account_profile_revision":1,"group_profile_revision":1,"evidence_ref":"otter-local:test","checks":["company","account","group_name","group_context"]});
    let prep = f.device_call(scope.id, prepare).await;
    let permit = f
        .device_call(
            scope.id,
            serde_json::to_value(Fixture::permit_input(&claim, &prep)).unwrap(),
        )
        .await;
    assert!(permit["invocation_id"].is_string());
    assert!(
        f.repo
            .execute(
                f.principal.clone(),
                scope.clone(),
                "no-second-permit",
                Fixture::permit_input(&claim, &prep),
                true,
                "https://example.test"
            )
            .await
            .is_err()
    );
    let unknown=f.device_call(scope.id,json!({"operation":"result","invocation_id":permit["invocation_id"],"permit_token":permit["permit_token"],"event_id":Uuid::new_v4(),"result":"not_observed","evidence_ref":"otter-local:test","observed_account_id":f.sender,"observed_group_id":f.group,"content_digest":claim["content_digest"],"local_state":"not_observed","reported_clicked_at":null})).await;
    assert_eq!(unknown["state"], "unknown");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_delivery_strategy_runs")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore]
async fn visible_profile_change_invalidates_frozen_run_without_rewriting_it() {
    let f = visible_fixture().await;
    let (run, task) = visible_run(&f).await;
    f.confirm(&task).await;
    let before: Value =
        sqlx::query_scalar("SELECT snapshot FROM mobile_delivery_items WHERE task_id=$1")
            .bind(uuid(&run, "task_id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    f.call(Some(f.school),Some(f.group),json!({"operation":"update_group_profile","expected_revision":3,"name":"机器人测试(2)","visible_profile":{"group_type":"external"},"reason":"real name changed; parentheses preserved"})).await;
    let after: Value =
        sqlx::query_scalar("SELECT snapshot FROM mobile_delivery_items WHERE task_id=$1")
            .bind(uuid(&run, "task_id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let poll = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(poll["claim_id"].is_null());
    let review: Value = sqlx::query_scalar("SELECT to_jsonb(g) FROM delivery_groups g WHERE id=$1")
        .bind(f.group)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(review["name"], "机器人测试(2)");
    assert_eq!(review["verification_state"], "pending");
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn configured_order_is_transactional_and_waits_after_authoritative_ack() {
    let mut f = visible_fixture().await;
    f.principal = f.principal.for_protocol(3);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-configuration-v3.json"
    ))
    .unwrap();
    let config = f
        .call(
            None,
            None,
            json!({"operation":"create_configuration","input":fixture["configuration"]}),
        )
        .await;
    let release=f.call(None,Some(uuid(&config,"id")),json!({"operation":"publish_configuration","expected_revision":1,"school_id":f.school,"device_id":f.device,"evidence_ref":"synthetic:publish"})).await;
    let resolved=f.device_call(None,json!({"operation":"resolve_configuration","package_name":"com.wekeeper.fixture.wecom","version_name":"synthetic","version_code":1,"engine_version":"otter-ui/1","capabilities":ENGINE_CAPABILITIES})).await;
    assert_eq!(resolved["id"], release["id"]);
    let group=f.call(Some(f.school),None,json!({"operation":"create_group","client_kind":"wecom","identity_mode":"ui_visible","name":"第二个测试群","visible_profile":{"group_type":"external"},"identity_evidence_ref":"synthetic:group"})).await;
    let second = uuid(&group, "id");
    f.call(Some(f.school),Some(second),json!({"operation":"review_group_profile","expected_revision":1,"evidence_ref":"synthetic:review"})).await;
    f.call(Some(f.school),Some(second),json!({"operation":"bind_group","sender_account_id":f.sender,"mapping_revision":2,"reason":"synthetic:binding"})).await;
    let strategy=f.call(Some(f.school),None,json!({"operation":"create_strategy","name":"有序双群","sender_account_id":f.sender,"device_id":f.device,"delivery_mode":"ordered_groups","targets":[{"group_id":f.group},{"group_id":second}],"configuration_release_id":release["id"],"text_content":"测试顺序","trigger_mode":"manual_once","enabled":true})).await;
    let run = f
        .call(
            Some(f.school),
            Some(uuid(&strategy, "id")),
            json!({"operation":"run_strategy","expected_revision":1}),
        )
        .await;
    let task = uuid(&run, "task_id");
    let preview = f
        .call(
            Some(f.school),
            Some(task),
            json!({"operation":"preview","expected_revision":1}),
        )
        .await;
    assert_eq!(preview["items"].as_array().unwrap().len(), 2);
    assert_eq!(preview["requires_shared_text_scope_acceptance"], true);
    f.call(Some(f.school),Some(task),json!({"operation":"confirm","expected_revision":1,"preview_digest":preview["preview_digest"],"content_review":true,"scope_acceptance":true,"shared_text_scope_acceptance":true,"substantive_difference_reviewed":false,"evidence_ref":"synthetic:confirmed-same-text"})).await;
    let claim = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(claim["sequence"]["position"], 1);
    assert_eq!(claim["group_snapshot"]["id"], f.group.to_string());
    let prep=f.device_call(Some(uuid(&claim,"id")),json!({"operation":"prepare","fence":claim["fence"],"snapshot_digest":claim["snapshot_digest"],"observed_account_id":f.sender,"observed_group_id":f.group,"client_version":"synthetic","script_version":"otter-ui/1","baseline_version":"synthetic-1","checks_passed":true,"verification":{"method":"visible_ui","account_profile_revision":1,"group_profile_revision":1,"evidence_ref":"synthetic:visible","checks":["company","account","group_name","group_context"]}})).await;
    let permit = f
        .device_call(
            Some(uuid(&claim, "id")),
            serde_json::to_value(Fixture::permit_input(&claim, &prep)).unwrap(),
        )
        .await;
    let event = json!({"operation":"result","invocation_id":permit["invocation_id"],"permit_token":permit["permit_token"],"event_id":Uuid::new_v4(),"result":"ui_observed_sent","evidence_ref":"synthetic:observed","observed_account_id":f.sender,"observed_group_id":f.group,"content_digest":claim["content_digest"],"local_state":"sent_bubble_observed","reported_clicked_at":permit["server_time"],"verification":{"method":"visible_ui","account_profile_revision":1,"group_profile_revision":1,"evidence_ref":"synthetic:visible","checks":["company","account","group_name","group_context"]}});
    let ack = f.device_call(Some(uuid(&claim, "id")), event.clone()).await;
    assert_eq!(ack["state"], "ui_observed_sent");
    assert_eq!(ack["run_progress"]["current_position"], 2);
    let replay = f.device_call(Some(uuid(&claim, "id")), event).await;
    assert_eq!(replay["run_progress"]["current_position"], 2);
    let wait = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(wait["reason"], "group_interval_active");
    assert!(wait["claim_id"].is_null());
    sqlx::query("UPDATE mobile_delivery_run_progress SET next_eligible_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(uuid(&run,"id")).execute(&f.pool).await.unwrap();
    sqlx::query("UPDATE delivery_usage SET observed_at=clock_timestamp()-interval '2 minutes' WHERE source_id=$1").bind(uuid(&claim,"item_id")).execute(&f.pool).await.unwrap();
    let next = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(next["sequence"]["position"], 2);
    assert_eq!(next["group_snapshot"]["id"], second.to_string());
}

async fn short_code(f: &Fixture) -> String {
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM execution_devices WHERE id=$1")
        .bind(f.device)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    f.call(
        None,
        Some(f.device),
        json!({"operation":"enrollment","expected_revision":revision}),
    )
    .await["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn pair_code(
    f: &Fixture,
    code: &str,
    instance: &str,
    key: &str,
    version: u32,
    device: Option<Uuid>,
) -> Result<Value, Error> {
    let mut input = json!({"operation":"enroll","code":code,"client_instance_id":instance,"protocol_version":version});
    if let Some(device) = device {
        input["device_id"] = json!(device);
    }
    f.repo
        .execute(
            Principal::Enrollment.for_protocol(version),
            Scope {
                school: None,
                id: None,
            },
            key,
            command(input),
            true,
            "https://example.test",
        )
        .await
}

#[test]
fn enrollment_contract_accepts_code_only_and_keeps_optional_device_constraint() {
    let input = json!({"operation":"enroll","code":"00123456","client_instance_id":"phone","protocol_version":3});
    assert_eq!(serde_json::to_value(command(input.clone())).unwrap(), input);
    let mut legacy = input;
    legacy["device_id"] = json!(Uuid::new_v4());
    assert_eq!(
        serde_json::to_value(command(legacy.clone())).unwrap(),
        legacy
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn new_device_pairs_without_enable_step_and_state_changes_keep_revision_and_revocation_guards()
 {
    let f = setup().await;
    let created = f
        .call(
            None,
            None,
            json!({"operation":"create_device","name":"New phone"}),
        )
        .await;
    assert_eq!(created["enabled"], true);
    let d = uuid(&created, "id");
    let code = f
        .call(
            None,
            Some(d),
            json!({"operation":"enrollment","expected_revision":1}),
        )
        .await;
    let credentials = pair_code(
        &f,
        code["code"].as_str().unwrap(),
        "new-install",
        "new-pair",
        5,
        None,
    )
    .await
    .unwrap();
    let token = credentials["device_token"].as_str().unwrap();
    let p = f.repo.authenticate(token).await.unwrap().for_protocol(5);
    let session = f
        .repo
        .query(
            p,
            Scope {
                school: None,
                id: None,
            },
            Query::Session,
            Page::default(),
        )
        .await
        .unwrap();
    assert!(
        session["binding"].is_null(),
        "connection must not require account binding or environment"
    );
    let update = |revision, enabled| {
        command(
            json!({"operation":"update_device","expected_revision":revision,"enabled":enabled,"reason":"Administrator reviewed state"}),
        )
    };
    let scope = Scope {
        school: None,
        id: Some(d),
    };
    let stale = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "stale-state",
            update(1, false),
            false,
            "https://example.test",
        )
        .await;
    assert!(matches!(stale, Err(Error::Policy("revision_conflict"))));
    f.repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "disable-state",
            update(2, false),
            false,
            "https://example.test",
        )
        .await
        .unwrap();
    assert!(matches!(
        f.repo.authenticate(token).await,
        Err(Error::Unauthorized)
    ));
    f.repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "enable-state",
            update(3, true),
            false,
            "https://example.test",
        )
        .await
        .unwrap();
    f.repo.authenticate(token).await.unwrap();
    f.call(
        None,
        Some(d),
        json!({"operation":"revoke","expected_revision":4,"reason":"Device lost"}),
    )
    .await;
    f.repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            "enable-after-revoke",
            update(5, true),
            false,
            "https://example.test",
        )
        .await
        .unwrap();
    assert!(
        matches!(f.repo.authenticate(token).await, Err(Error::Unauthorized)),
        "enabling must not revive revoked credentials"
    );
}

#[test]
fn pairing_errors_use_uniform_unauthorized_and_backoff_responses() {
    use axum::{http::StatusCode, response::IntoResponse};
    assert_eq!(
        Error::Policy("pairing_code_invalid")
            .into_response()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = Error::Policy("pairing_rate_limited").into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "600");
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn short_pairing_codes_are_unique_expiring_one_time_and_resolve_device() {
    let f = setup().await;
    let old = short_code(&f).await;
    let current = short_code(&f).await;
    assert_eq!(current.len(), 8);
    assert!(current.bytes().all(|b| b.is_ascii_digit()));
    assert_ne!(old, current);
    assert!(matches!(
        pair_code(&f, &old, "old", "old", 3, None).await,
        Err(Error::Policy("pairing_code_invalid"))
    ));
    let other = f
        .call(
            None,
            None,
            json!({"operation":"create_device","name":"Other phone"}),
        )
        .await;
    assert!(matches!(
        pair_code(
            &f,
            &current,
            "wrong-device",
            "wrong-device",
            3,
            Some(uuid(&other, "id"))
        )
        .await,
        Err(Error::Policy("pairing_code_invalid"))
    ));
    let grouped = format!("{} {}", &current[..4], &current[4..]);
    let (a, b) = tokio::join!(
        pair_code(&f, &grouped, "new-phone", "pair-a", 3, None),
        pair_code(&f, &grouped, "new-phone", "pair-b", 3, None)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let (credential, replay_key) = if let Ok(value) = a {
        (value, "pair-a")
    } else {
        (b.unwrap(), "pair-b")
    };
    assert_eq!(credential["id"], f.device.to_string());
    assert_eq!(credential["protocol_version"], 3);
    assert!(credential["device_token"].is_string());
    let replay = pair_code(&f, &grouped, "new-phone", replay_key, 3, None)
        .await
        .unwrap();
    assert_eq!(replay["secret_replay_unavailable"], true);
    assert!(replay.get("device_token").is_none());
    assert!(matches!(
        pair_code(&f, &current, "new-phone", "consumed", 3, None).await,
        Err(Error::Policy("pairing_code_invalid"))
    ));
    let expired = short_code(&f).await;
    sqlx::query("UPDATE device_enrollments SET expires_at=clock_timestamp()-interval '1 second' WHERE device_id=$1")
        .bind(f.device).execute(&f.pool).await.unwrap();
    assert!(matches!(
        pair_code(&f, &expired, "expired", "expired", 1, None).await,
        Err(Error::Policy("pairing_code_invalid"))
    ));
    // A pre-upgrade long code and ID still work until their original expiry.
    let legacy = "a".repeat(64);
    sqlx::query("UPDATE device_enrollments SET code_hash=$2,expires_at=clock_timestamp()+interval '1 minute' WHERE device_id=$1 AND consumed_at IS NULL")
        .bind(f.device).bind(wekeeper_api::business::hash(&legacy)).execute(&f.pool).await.unwrap();
    assert!(
        pair_code(&f, &legacy, "legacy-phone", "legacy", 1, Some(f.device))
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn pairing_failed_guesses_commit_budgets_across_codes_instances_and_protocols() {
    let f = setup().await;
    for n in 0..5 {
        assert!(matches!(
            pair_code(
                &f,
                &format!("bad-{n}"),
                "guesser",
                &format!("key-{n}"),
                1,
                None
            )
            .await,
            Err(Error::Policy("pairing_code_invalid"))
        ));
    }
    assert!(matches!(
        pair_code(&f, "12345678", "guesser", "limited", 3, None).await,
        Err(Error::Policy("pairing_rate_limited"))
    ));
    let code = short_code(&f).await;
    // Rotate instance IDs and protocol versions while deliberately supplying a
    // different device. Failed transactions must not roll back code counters.
    for n in 0..5 {
        assert!(matches!(
            pair_code(
                &f,
                &code,
                &format!("guess-{n}"),
                "guess",
                1 + n % 3,
                Some(Uuid::new_v4())
            )
            .await,
            Err(Error::Policy("pairing_code_invalid"))
        ));
    }
    assert!(matches!(
        pair_code(&f, &code, "fresh-instance", "blocked-code", 3, None).await,
        Err(Error::Policy("pairing_rate_limited"))
    ));
    sqlx::query(
        "UPDATE device_pairing_attempt_windows SET reset_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    assert!(
        pair_code(&f, &code, "fresh-instance", "after-window", 3, None)
            .await
            .is_ok()
    );
    sqlx::query("UPDATE device_pairing_attempt_windows SET requests=100 WHERE scope='global'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        pair_code(&f, "87654321", "another-instance", "global", 2, None).await,
        Err(Error::Policy("pairing_rate_limited"))
    ));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn pairing_preserves_leading_zeroes_and_only_stores_code_hashes() {
    let f = setup().await;
    let generated = short_code(&f).await;
    let code_hash = wekeeper_api::business::hash("00123456");
    sqlx::query("UPDATE device_enrollments SET code_hash=$2 WHERE code_hash=$1")
        .bind(wekeeper_api::business::hash(&generated))
        .bind(&code_hash)
        .execute(&f.pool)
        .await
        .unwrap();
    let credential = pair_code(&f, "0012-3456", "leading-zero", "zero", 2, None)
        .await
        .unwrap();
    assert_eq!(credential["id"], f.device.to_string());
    let stored: Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM device_enrollments e WHERE code_hash=$1")
            .bind(code_hash)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!stored.to_string().contains("00123456"));
    let operation: Value = sqlx::query_scalar(
        "SELECT response FROM mobile_delivery_operations WHERE idempotency_key='zero'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(operation["secret_replay_unavailable"], true);
    assert!(operation.get("device_token").is_none());
}

// Synthetic protocol coverage only; these fixtures never seed a real supported environment.
async fn grant_test_authorization(f: &Fixture, revision: i64, maintenance: bool) -> Value {
    let context = f
        .repo
        .query(
            f.principal.clone(),
            Scope {
                school: None,
                id: None,
            },
            Query::Automation(AutomationQuery::AuthorizationContext),
            Page::default(),
        )
        .await
        .unwrap();
    let mut scope = context["scope"].clone();
    scope["origin"] = json!("https://example.test");
    let receipt = json!({"authorization_id":Uuid::new_v4(),"revision":revision,"state":"effective","notice_version":"internal_business/1","notice_digest":"a".repeat(64),"identity_assurance":"local_action","granted_at":chrono::Utc::now(),"revoked_at":null,"maintenance":maintenance,"scope":scope});
    f.device_call(None,json!({"operation":"automation","input":{"action":"record_authorization","receipt":receipt}})).await;
    receipt
}
async fn automation_fixture() -> (Fixture, Value, Value) {
    let mut f = visible_fixture().await;
    f.principal = f.principal.for_protocol(4);
    grant_test_authorization(&f, 1, false).await;
    let wire: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-environment-v4.json"
    ))
    .unwrap();
    let e=f.device_call(None,json!({"operation":"automation","input":{"action":"report_environment","report":wire["data"]["attributes"]["report"]}})).await;
    let b=f.call(Some(f.school),None,json!({"operation":"automation","input":{"action":"bind_application","device_id":f.device,"app_id":"00000000-0000-0000-0000-000000000001","environment_id":e["id"],"instance_ref":"synthetic-instance","expected_revision":0}})).await;
    grant_test_authorization(&f, 2, true).await;
    (f, e, b)
}
async fn automation_job(f: &Fixture, b: &Value, kind: &str, components: Value) -> Value {
    f.call(Some(f.school),None,json!({"operation":"automation","input":{"action":"request_validation","device_id":f.device,"app_binding_id":b["id"],"kind":kind,"component_ids":components}})).await
}
async fn automation_finish(
    f: &Fixture,
    job: &Value,
    path: Option<&str>,
    role_matches: Value,
    timing: Value,
) -> Value {
    let j=f.device_call(Some(uuid(job,"id")),json!({"operation":"automation","input":{"action":"accept_validation","expected_revision":job["revision"],"generation":job["generation"]}})).await;
    f.device_call(Some(uuid(&j,"id")),json!({"operation":"automation","input":{"action":"report_validation","event":{"event_id":Uuid::new_v4(),"generation":j["generation"],"expected_revision":j["revision"],"stage":"ready","outcome":"succeeded","path":path,"role_matches":role_matches,"timing":timing}}})).await
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_validations_are_owned_cas_cancel_safe_and_never_send() {
    let (f, _, b) = automation_fixture().await;
    let job = automation_job(&f, &b, "measure_manual", json!([])).await;
    let accepted=f.device_call(Some(uuid(&job,"id")),json!({"operation":"automation","input":{"action":"accept_validation","expected_revision":job["revision"],"generation":job["generation"]}})).await;
    let wrong=f.repo.execute(f.principal.clone(),Scope{school:None,id:Some(uuid(&job,"id"))},"stale-generation",command(json!({"operation":"automation","input":{"action":"accept_validation","expected_revision":job["revision"],"generation":Uuid::new_v4()}})),true,"").await;
    assert!(wrong.is_err());
    let cancelled=f.call(Some(f.school),Some(uuid(&job,"id")),json!({"operation":"automation","input":{"action":"cancel_validation","expected_revision":accepted["revision"]}})).await;
    let event = json!({"operation":"automation","input":{"action":"report_validation","event":{"event_id":Uuid::new_v4(),"generation":job["generation"],"expected_revision":accepted["revision"],"stage":"ready","outcome":"succeeded","role_matches":{},"path":null,"timing":{"group_interval_ms":10000,"navigation_ms":100,"input_codepoint_ms":100,"input_settle_ms":100,"observation_ms":100}}}});
    let late = f.device_call(Some(uuid(&job, "id")), event.clone()).await;
    assert_eq!(late, cancelled);
    let replay = f.device_call(Some(uuid(&job, "id")), event).await;
    assert_eq!(late, replay);
    let applied: bool =
        sqlx::query_scalar("SELECT applied FROM device_validation_events WHERE job_id=$1")
            .bind(uuid(&job, "id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!applied);
    let sends: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_send_permits")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(sends, 0);
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM device_validation_events WHERE job_id=$1")
            .bind(uuid(&job, "id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(events, 1);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automation_compiles_preview_freezes_manifest_and_requires_readiness_before_claim() {
    let (f, e, b) = automation_fixture().await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/mobile-configuration-v3.json"
    ))
    .unwrap();
    let configuration = &fixture["configuration"];
    let marker = configuration["rules"]["pages"]["home"].clone();
    let role = configuration["rules"]["controls"]["send"].clone();
    let search = configuration["rules"]["controls"]["search_input"].clone();
    let launcher=f.call(None,None,json!({"operation":"automation","input":{"action":"create_component","name":"SYNTHETIC launcher","body":{"kind":"launcher","flow":{"client":e["body"]["launcher"],"pages":{"home":marker,"search":marker,"app":marker},"roles":{"application":role,"search":search},"search_steps":[{"from":"home","to":"home","operation":{"action":"home"},"delay_ms":100},{"from":"home","to":"search","operation":{"action":"search_application","role":"search"},"delay_ms":100},{"from":"search","to":"app","operation":{"action":"click","role":"application"},"delay_ms":100},{"from":"app","to":"app","operation":{"action":"verify_application"},"delay_ms":100}],"all_apps_steps":[],"max_steps":10,"timeout_ms":60000,"max_scrolls":10,"local_application_results_only":true}}}})).await;
    let workflow=f.call(None,None,json!({"operation":"automation","input":{"action":"create_component","name":"SYNTHETIC workflow","body":{"kind":"workflow","rules":configuration["rules"]}}})).await;
    let policy=f.call(None,None,json!({"operation":"automation","input":{"action":"create_component","name":"SYNTHETIC policy","body":{"kind":"policy","policy":configuration["policy"]}}})).await;
    let opened = automation_finish(
        &f,
        &automation_job(&f, &b, "open_application", json!([launcher["id"]])).await,
        Some("search"),
        json!({}),
        Value::Null,
    )
    .await;
    let mut matches = serde_json::Map::new();
    for k in configuration["rules"]["controls"]
        .as_object()
        .unwrap()
        .keys()
    {
        matches.insert(k.clone(), json!(1));
    }
    for k in configuration["rules"]["pages"].as_object().unwrap().keys() {
        matches.insert(format!("page:{k}"), json!(1));
    }
    let inspected = automation_finish(
        &f,
        &automation_job(
            &f,
            &b,
            "inspect_roles",
            json!([workflow["id"], policy["id"]]),
        )
        .await,
        None,
        Value::Object(matches),
        Value::Null,
    )
    .await;
    let base = &configuration["baseline"];
    let timed=automation_finish(&f,&automation_job(&f,&b,"measure_manual",json!([])).await,None,json!({}),json!({"group_interval_ms":base["human_group_min_interval_ms"],"navigation_ms":base["navigation_min_ms"],"input_codepoint_ms":base["input_codepoint_min_ms"],"input_settle_ms":base["input_settle_min_ms"],"observation_ms":base["observation_min_ms"]})).await;
    // Fixture policy needs three distinct manual samples, each backed by a phone acceptance.
    let mut samples = vec![timed.clone()];
    for _ in 1..configuration["policy"]["minimum_manual_samples"]
        .as_u64()
        .unwrap()
    {
        samples.push(
            automation_finish(
                &f,
                &automation_job(&f, &b, "measure_manual", json!([])).await,
                None,
                json!({}),
                timed["result"]["timing"].clone(),
            )
            .await,
        );
    }
    let baseline=f.call(None,None,json!({"operation":"automation","input":{"action":"create_component","name":"SYNTHETIC timing","body":{"kind":"baseline","validation_ids":samples.iter().map(|v|v["id"].clone()).collect::<Vec<_>>()}}})).await;
    for (component, validation) in [
        (&launcher, &opened),
        (&workflow, &inspected),
        (&policy, &inspected),
        (&baseline, &timed),
    ] {
        f.call(None,Some(uuid(component,"id")),json!({"operation":"automation","input":{"action":"publish_component","school_id":f.school,"device_id":f.device,"validation_id":validation["id"],"expected_revision":1}})).await;
    }
    // School administrators do not need platform maintenance privileges to preview/confirm a business.
    sqlx::query("UPDATE accounts SET platform_admin=false WHERE id=$1")
        .bind(f.actor)
        .execute(&f.pool)
        .await
        .unwrap();
    let preview=f.call(Some(f.school),None,json!({"operation":"automation","input":{"action":"preview_business","input":{"app_binding_id":b["id"],"capability_id":"00000000-0000-0000-0000-000000000011","name":"SYNTHETIC business","group_ids":[f.group],"text_content":"测试 v4","interval_seconds":configuration["policy"]["default_group_interval_ms"].as_i64().unwrap()/1000,"schedule":"manual_once"}}})).await;
    let task: Value = sqlx::query_scalar("SELECT to_jsonb(t) FROM mobile_delivery_tasks t JOIN mobile_delivery_strategy_runs r ON r.task_id=t.id WHERE r.id=$1")
        .bind(uuid(&preview, "run_id")).fetch_one(&f.pool).await.unwrap();
    let bypass = f.repo.execute(Principal::Actor(f.actor), Scope { school: Some(f.school), id: Some(uuid(&task, "id")) }, "legacy-confirm-bypass", command(json!({"operation":"confirm","expected_revision":task["revision"],"preview_digest":preview["task_preview"]["preview_digest"],"content_review":true,"scope_acceptance":true,"substantive_difference_reviewed":true,"shared_text_scope_acceptance":true,"evidence_ref":"synthetic bypass"})), true, "https://example.test").await;
    assert!(matches!(bypass, Err(Error::Invalid(_))));
    // Refreshing the same environment may continue frozen runs, but invalidates an unconfirmed preview.
    let mut wire: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-environment-v4.json"
    ))
    .unwrap();
    wire["data"]["attributes"]["report"]["generation"] = json!(2);
    f.device_call(None,json!({"operation":"automation","input":{"action":"report_environment","report":wire["data"]["attributes"]["report"]}})).await;
    let stale = f.repo.execute(Principal::Actor(f.actor), Scope { school: Some(f.school), id: Some(uuid(&preview, "id")) }, "stale-environment-preview", command(json!({"operation":"automation","input":{"action":"confirm_business","expected_revision":1,"preview_digest":preview["preview_digest"],"reviewed":true}})), true, "https://example.test").await;
    assert!(matches!(stale, Err(Error::Policy("revision_conflict"))));
    let preview=f.call(Some(f.school),None,json!({"operation":"automation","input":{"action":"preview_business","input":preview["input"]}})).await;
    let run=f.call(Some(f.school),Some(uuid(&preview,"id")),json!({"operation":"automation","input":{"action":"confirm_business","expected_revision":1,"preview_digest":preview["preview_digest"],"reviewed":true}})).await;
    let old = f
        .repo
        .execute(
            f.principal.clone().for_protocol(3),
            Scope {
                school: None,
                id: None,
            },
            "old-client",
            command(json!({"operation":"claim","binding_revision":f.binding_revision})),
            true,
            "",
        )
        .await;
    assert!(
        old.is_err(),
        "a device with standing authorization must not acquire new work through v3"
    );
    let no_ticket = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(no_ticket["claim_id"].is_null());
    // An optional maintenance request outside the standing scope must not monopolize ordinary business.
    grant_test_authorization(&f, 3, false).await;
    sqlx::query("UPDATE accounts SET platform_admin=true WHERE id=$1")
        .bind(f.actor)
        .execute(&f.pool)
        .await
        .unwrap();
    let waiting_maintenance = automation_job(&f, &b, "measure_manual", json!([])).await;
    sqlx::query("UPDATE accounts SET platform_admin=false WHERE id=$1")
        .bind(f.actor)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(waiting_maintenance["state"], "queued");
    // Untouched business outside the local scope is not selected for UI preparation.
    let mut environment_only = grant_test_authorization(&f, 4, false).await;
    environment_only["revision"] = json!(5);
    environment_only["scope"]["schools"][0]["applications"] = json!([]);
    f.device_call(None,json!({"operation":"automation","input":{"action":"record_authorization","receipt":environment_only}})).await;
    let waiting = f
        .device_call(
            None,
            json!({"operation":"automation","input":{"action":"prepare_execution"}}),
        )
        .await;
    assert_eq!(waiting["reason"], "no_ready_task");
    grant_test_authorization(&f, 6, false).await;
    let ticket = f
        .device_call(
            None,
            json!({"operation":"automation","input":{"action":"prepare_execution"}}),
        )
        .await;
    assert_eq!(ticket["ready"], false);
    let frozen: Value =
        sqlx::query_scalar("SELECT body FROM automation_execution_manifests WHERE id=$1")
            .bind(uuid(&run, "automation_manifest_id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(frozen["business"]["text_content"], "测试 v4");
    f.device_call(Some(uuid(&ticket,"id")),json!({"operation":"automation","input":{"action":"application_ready","generation":ticket["generation"],"manifest_digest":ticket["manifest"]["content_digest"],"environment_id":ticket["environment_id"]}})).await;
    let claim = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(claim["requirements"]["protocol_version"], 4);
    assert_eq!(
        claim["automation_manifest"]["id"],
        run["automation_manifest_id"]
    );
    let permits: i64 = sqlx::query_scalar("SELECT count(*) FROM mobile_send_permits")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(permits, 0);
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn standing_authorization_revoke_fences_late_grants_and_decline_never_accepts() {
    let (f, _, b) = automation_fixture().await;
    let job = automation_job(&f, &b, "inspect_environment", json!([])).await;
    let rejected=f.device_call(Some(uuid(&job,"id")),json!({"operation":"automation","input":{"action":"reject_validation","expected_revision":job["revision"],"generation":job["generation"]}})).await;
    assert_eq!(rejected["state"], "declined");
    assert_eq!(rejected["decision_source"], "local_decline");
    assert!(rejected["accepted_at"].is_null());
    let r = grant_test_authorization(&f, 3, false).await;
    let job = automation_job(&f, &b, "inspect_environment", json!([])).await;
    let accepted=f.device_call(Some(uuid(&job,"id")),json!({"operation":"automation","input":{"action":"accept_standing_validation","expected_revision":job["revision"],"generation":job["generation"],"authorization_id":r["authorization_id"]}})).await;
    assert_eq!(accepted["decision_source"], "standing_authorization");
    assert_eq!(accepted["authorization_id"], r["authorization_id"]);
    let mut revoked = r.clone();
    revoked["state"] = json!("revoked");
    revoked["revision"] = json!(4);
    revoked["revoked_at"] = json!(chrono::Utc::now());
    f.device_call(None,json!({"operation":"automation","input":{"action":"record_authorization","receipt":revoked}})).await;
    let late=f.repo.execute(f.principal.clone(),Scope{school:None,id:None},"late-grant",command(json!({"operation":"automation","input":{"action":"record_authorization","receipt":r}})),true,"https://example.test").await;
    assert!(late.is_err());
    for protocol in [1, 3, 4] {
        let blocked = f
            .repo
            .execute(
                f.principal.clone().for_protocol(protocol),
                Scope {
                    school: None,
                    id: None,
                },
                &format!("revoked-claim-{protocol}"),
                command(json!({"operation":"claim","binding_revision":f.binding_revision})),
                true,
                "https://example.test",
            )
            .await;
        assert!(
            blocked.is_err(),
            "revoked receipt must also fence downgraded clients"
        );
    }

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM device_validation_events WHERE job_id=$1")
            .bind(uuid(&rejected, "id"))
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

/// All rows and evidence in this test are synthetic. No production validation is seeded.
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn dispatch_v5_queues_and_prepares_without_environment_tables() {
    let mut f = visible_fixture().await;
    f.principal = f.principal.for_protocol(5);
    // Make accidental old environment/binding SQL fail, instead of merely returning no rows.
    sqlx::query(
        "ALTER TABLE device_environment_snapshots RENAME TO forbidden_environment_snapshots",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE device_app_bindings RENAME TO forbidden_app_bindings")
        .execute(&f.pool)
        .await
        .unwrap();
    let authorization = Uuid::new_v4();
    f.device_call(None,json!({"operation":"dispatch","input":{"action":"authorization","receipt":{
        "authorization_id":authorization,"revision":1,"state":"effective","granted_at":chrono::Utc::now(),"revoked_at":null,
        "scope":{"format":"business_application_scope/2","origin":"https://example.test","device_id":f.device,"sender_account_id":f.sender,"schools":[f.school],"app_key":"wecom","capability":"group_text"}
    }}})).await;
    let mut generic: Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/automation-dispatch-flow-v5.json"
    ))
    .unwrap();
    let timing = json!({"group_interval_ms":generic["configuration"]["baseline"]["human_group_min_interval_ms"],"navigation_ms":generic["configuration"]["baseline"]["navigation_min_ms"],"input_codepoint_ms":generic["configuration"]["baseline"]["input_codepoint_min_ms"],"input_settle_ms":generic["configuration"]["baseline"]["input_settle_min_ms"],"observation_ms":generic["configuration"]["baseline"]["observation_min_ms"]});
    // Test fixture helper carries the independent wire evidence dimensions.
    #[allow(clippy::too_many_arguments)]
    async fn validation(
        f: &Fixture,
        auth: Uuid,
        flow: Option<Uuid>,
        kind: &str,
        launcher: Option<usize>,
        roles: Value,
        timing: Value,
        path: Value,
    ) -> Value {
        let manual = if flow.is_none() {
            json!({"label":"SYNTHETIC","client":{"package_name":"com.wekeeper.fixture.wecom"}})
        } else {
            Value::Null
        };
        let j=f.call(Some(f.school),None,json!({"operation":"dispatch","input":{"action":"request_validation","device_id":f.device,"flow_id":flow,"manual_application":manual,"kind":kind,"path_key":"generic","launcher_index":launcher}})).await;
        let accepted=f.device_call(Some(uuid(&j,"id")),json!({"operation":"dispatch","input":{"action":"accept_validation","expected_revision":j["revision"],"generation":j["generation"],"authorization_id":auth}})).await;
        assert_eq!(accepted["state"], "accepted");
        f.device_call(Some(uuid(&j,"id")),json!({"operation":"dispatch","input":{"action":"validation_event","event":{"event_id":Uuid::new_v4(),"generation":accepted["generation"],"expected_revision":accepted["revision"],"stage":"ready","outcome":"succeeded","role_matches":roles,"timing":timing,"path":path}}})).await
    }
    let mut measurements = Vec::new();
    for _ in 0..3 {
        measurements.push(uuid(
            &validation(
                &f,
                authorization,
                None,
                "measure_manual",
                None,
                json!({}),
                timing.clone(),
                Value::Null,
            )
            .await,
            "id",
        ));
    }
    let pacing=f.call(None,None,json!({"operation":"dispatch","input":{"action":"create_pacing","name":"SYNTHETIC baseline","school_id":f.school,"validation_ids":measurements}})).await;
    generic["configuration"]["baseline"] = pacing["baseline"].clone();
    let flow=f.call(None,None,json!({"operation":"dispatch","input":{"action":"create_flow","flow":generic,"pacing_profile_id":pacing["id"]}})).await;
    let mut roles = serde_json::Map::new();
    for k in generic["configuration"]["rules"]["controls"]
        .as_object()
        .unwrap()
        .keys()
    {
        roles.insert(k.clone(), json!(1));
    }
    for k in generic["configuration"]["rules"]["pages"]
        .as_object()
        .unwrap()
        .keys()
    {
        roles.insert(format!("page:{k}"), json!(1));
    }
    let inspected = validation(
        &f,
        authorization,
        Some(uuid(&flow, "id")),
        "inspect_roles",
        None,
        Value::Object(roles),
        Value::Null,
        Value::Null,
    )
    .await;
    for (index, launcher) in generic["launchers"].as_array().unwrap().iter().enumerate() {
        for (path, key) in [("search", "search_steps"), ("all_apps", "all_apps_steps")] {
            if !launcher[key].as_array().unwrap().is_empty() {
                validation(
                    &f,
                    authorization,
                    Some(uuid(&flow, "id")),
                    "open_application",
                    Some(index),
                    json!({}),
                    Value::Null,
                    json!(path),
                )
                .await;
            }
        }
    }
    let release=f.call(None,Some(uuid(&flow,"id")),json!({"operation":"dispatch","input":{"action":"publish_flow","school_id":f.school,"validation_id":inspected["id"]}})).await;
    let assignments=f.call(Some(f.school),None,json!({"operation":"dispatch","input":{"action":"assign","device_ids":[f.device],"group_id":null,"release_id":release["id"]}})).await;
    // Still offline and registered as an old client: queueing must not require protocol/environment readiness.
    sqlx::query(
        "UPDATE execution_devices SET execution_protocol_version=2,last_seen_at=NULL WHERE id=$1",
    )
    .bind(f.device)
    .execute(&f.pool)
    .await
    .unwrap();
    f.call(Some(f.school),None,json!({"operation":"dispatch","input":{"action":"set_enabled","enabled":true,"reviewed_history":true}})).await;
    let batch=f.call(Some(f.school),None,json!({"operation":"dispatch","input":{"action":"preview","input":{"name":"SYNTHETIC batch","text_content":"测试 v5","interval_seconds":60,"targets":[{"assignment_id":assignments["assignments"][0]["id"],"group_ids":[f.group]}]}}})).await;
    let key = Uuid::new_v4().to_string();
    let confirm = command(
        json!({"operation":"dispatch","input":{"action":"confirm","preview_digest":batch["preview_digest"],"reviewed":true}}),
    );
    let scope = Scope {
        school: Some(f.school),
        id: Some(uuid(&batch, "id")),
    };
    let first = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope.clone(),
            &key,
            confirm.clone(),
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    let replay = f
        .repo
        .execute(
            Principal::Actor(f.actor),
            scope,
            &key,
            confirm,
            true,
            "https://example.test",
        )
        .await
        .unwrap();
    assert_eq!(first, replay);
    let inbox = f
        .repo
        .query(
            f.principal.clone(),
            Scope {
                school: None,
                id: None,
            },
            Query::Dispatch(DispatchQuery::Inbox),
            Page::default(),
        )
        .await
        .unwrap();
    let delivery = &inbox[0];
    assert_eq!(delivery["plan"]["body"]["protocol_version"], 5);
    assert!(delivery["plan"]["body"].get("environment_id").is_none());
    let did = uuid(delivery, "id");
    let digest = delivery["plan"]["content_digest"].clone();
    f.device_call(
        Some(did),
        json!({"operation":"dispatch","input":{"action":"receipt","plan_digest":digest}}),
    )
    .await;
    let wait = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert!(
        wait["claim_id"].is_null(),
        "a receipt is not readiness: {wait}"
    );
    let lease=f.device_call(Some(did),json!({"operation":"dispatch","input":{"action":"preparation_lease","plan_digest":digest,"authorization_id":authorization}})).await;
    f.device_call(Some(did),json!({"operation":"dispatch","input":{"action":"stage","generation":lease["generation"],"item_id":lease["item_id"],"stage":"ready","path_key":"generic"}})).await;
    let claim = f
        .device_call(
            None,
            json!({"operation":"claim","binding_revision":f.binding_revision}),
        )
        .await;
    assert_eq!(claim["requirements"]["protocol_version"], 5);
    assert!(claim["requirements"].get("client_version").is_none());
    assert_eq!(claim["dispatch_plan"]["content_digest"], digest);
    f.device_call(
        Some(uuid(&claim, "id")),
        json!({"operation":"abort","fence":claim["fence"],"reason":"synthetic-not-clicked"}),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM forbidden_environment_snapshots")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM forbidden_app_bindings")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn binding_eligibility_matches_projection_and_optional_notes_do_not_authorize() {
    let f = setup().await;
    let scope = Scope {
        school: None,
        id: Some(f.device),
    };
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM execution_devices WHERE id=$1")
        .bind(f.device)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let bound = f.call(None, Some(f.device), json!({"operation":"bind_device","expected_revision":revision,"sender_account_id":f.sender})).await;
    assert_eq!(bound["identity_evidence_ref"], "");
    let revision = bound["revision"].as_i64().unwrap();
    for (mutation, policy) in [
        (
            "UPDATE sending_accounts SET enabled=false",
            "binding_account_disabled",
        ),
        (
            "UPDATE sending_accounts SET enabled=true; UPDATE schools SET active=false",
            "binding_school_authorization_required",
        ),
        (
            "UPDATE schools SET active=true; UPDATE school_memberships SET role='operator'",
            "binding_school_authorization_required",
        ),
        (
            "UPDATE school_memberships SET role='admin'; DELETE FROM sender_group_bindings",
            "binding_school_authorization_required",
        ),
    ] {
        sqlx::raw_sql(mutation).execute(&f.pool).await.unwrap();
        let accounts = f
            .repo
            .query(
                Principal::Actor(f.actor),
                Scope {
                    school: None,
                    id: None,
                },
                Query::Accounts,
                Page::default(),
            )
            .await
            .unwrap();
        let account = accounts
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == f.sender.to_string())
            .unwrap();
        assert_eq!(account["device_binding_eligibility"]["eligible"], false);
        let reason = account["device_binding_eligibility"]["reason"]
            .as_str()
            .unwrap();
        assert_eq!(format!("binding_{reason}"), policy);
        for note in ["", "我已经核对了真实身份"] {
            let result = f.repo.execute(Principal::Actor(f.actor), scope.clone(), &Uuid::new_v4().to_string(), command(json!({"operation":"bind_device","expected_revision":revision,"sender_account_id":f.sender,"identity_evidence_ref":note})), true, "https://example.test").await;
            assert!(matches!(result, Err(Error::Policy(code)) if code == policy));
        }
        let actual: i64 = sqlx::query_scalar("SELECT revision FROM execution_devices WHERE id=$1")
            .bind(f.device)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(actual, revision);
    }
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn device_control_is_independent_durable_and_fenced() {
    use wekeeper_api::distribution::{
        domain::device_control as control,
        south::{
            adapter::postgres::device_control::Repository as ControlRepository,
            port::device_control::Store as ControlStore,
        },
    };
    let f = setup().await;
    let repo = ControlRepository(f.pool.clone());
    let generation = match &f.principal {
        Principal::Device { generation, .. } => *generation,
        _ => panic!("device principal"),
    };
    let c = control::Credential {
        device_id: f.device,
        generation,
        token_hash: wekeeper_api::business::hash(&f.token),
    };
    sqlx::query("DELETE FROM device_account_bindings WHERE device_id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    let first = repo
        .device(
            &c,
            control::Operation::Open {
                transport: "https".into(),
            },
        )
        .await
        .unwrap();
    let pause = repo
        .enqueue(
            f.actor,
            f.device,
            "pause",
            control::CommandInput {
                kind: control::CommandKind::PauseExecution,
            },
        )
        .await
        .unwrap();
    let replay = repo
        .enqueue(
            f.actor,
            f.device,
            "pause",
            control::CommandInput {
                kind: control::CommandKind::PauseExecution,
            },
        )
        .await
        .unwrap();
    assert_eq!(pause["id"], replay["id"]);
    assert!(
        repo.enqueue(
            f.actor,
            f.device,
            "pause",
            control::CommandInput {
                kind: control::CommandKind::ResumeExecution
            }
        )
        .await
        .is_err()
    );
    let beat = || control::Pulse {
        connection_id: first.connection_id,
        epoch: first.epoch,
        heartbeat_id: Some(Uuid::new_v4()),
        readiness: "blocked".into(),
        block_reason: Some("device_locked".into()),
        receipts: vec![],
    };
    let frame = repo
        .device(&c, control::Operation::Pulse(beat()))
        .await
        .unwrap();
    assert!(frame.desired_paused);
    assert!(frame.commands.iter().any(|x| x.id == uuid(&pause, "id")));
    let p = repo.presence(f.actor, f.device).await.unwrap();
    assert_eq!(p["state"], "online");
    assert_eq!(p["block_reason"], "device_locked");
    let second = repo
        .device(
            &c,
            control::Operation::Open {
                transport: "wss".into(),
            },
        )
        .await
        .unwrap();
    assert!(second.epoch > first.epoch);
    assert!(matches!(
        repo.device(&c, control::Operation::Pulse(beat())).await,
        Err(Error::Policy("control_session_changed"))
    ));
    let mut receipt = beat();
    receipt.connection_id = second.connection_id;
    receipt.epoch = second.epoch;
    receipt.receipts = vec![control::Receipt {
        id: uuid(&pause, "id"),
        state: control::ReceiptState::Completed,
        result_code: None,
    }];
    let ack = repo
        .device(&c, control::Operation::Pulse(receipt.clone()))
        .await
        .unwrap();
    assert_eq!(ack.heartbeat_id, receipt.heartbeat_id);
    assert!(!ack.commands.iter().any(|x| x.id == uuid(&pause, "id")));
    repo.device(
        &c,
        control::Operation::Close {
            connection_id: first.connection_id,
            epoch: first.epoch,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        repo.presence(f.actor, f.device).await.unwrap()["state"],
        "online"
    );
    sqlx::query("UPDATE device_control_sessions SET last_heartbeat_at=clock_timestamp()-interval '66 seconds' WHERE device_id=$1").bind(f.device).execute(&f.pool).await.unwrap();
    assert_eq!(
        repo.presence(f.actor, f.device).await.unwrap()["state"],
        "offline"
    );
    // Candidate registration and activation survive repeat requests; only hashes are stored.
    let token = format!(
        "wk_device_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let hash = wekeeper_api::business::hash(&token);
    for _ in 0..2 {
        repo.device(
            &c,
            control::Operation::Renew {
                candidate_hash: hash.clone(),
            },
        )
        .await
        .unwrap();
    }
    let next = control::Credential {
        device_id: f.device,
        generation,
        token_hash: hash,
    };
    for _ in 0..2 {
        repo.device(&next, control::Operation::Activate)
            .await
            .unwrap();
    }
    let remaining:bool=sqlx::query_scalar("SELECT expires_at<=clock_timestamp()+interval '10 minutes' FROM device_credentials WHERE token_hash=$1").bind(&c.token_hash).fetch_one(&f.pool).await.unwrap();
    assert!(remaining);
    sqlx::query(
        "UPDATE execution_devices SET credential_generation=credential_generation+1 WHERE id=$1",
    )
    .bind(f.device)
    .execute(&f.pool)
    .await
    .unwrap();
    assert!(matches!(
        repo.device(
            &next,
            control::Operation::Open {
                transport: "https".into()
            }
        )
        .await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn setup_is_visible_before_consent_and_revocation_survives_binding_removal() {
    let mut f = visible_fixture().await;
    f.principal = f.principal.for_protocol(5);
    let scope = Scope {
        school: None,
        id: None,
    };
    let status = f
        .repo
        .query(
            f.principal.clone(),
            scope.clone(),
            Query::Dispatch(DispatchQuery::SetupStatus),
            Page::default(),
        )
        .await
        .unwrap();
    assert_eq!(status["sender_account_id"], json!(f.sender));
    assert_eq!(status["binding_revision"], json!(f.binding_revision));
    assert!(
        status["schools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == json!(f.school) && s["assignment_ready"] == false)
    );
    let mut receipt = json!({"authorization_id":Uuid::new_v4(),"revision":1,"state":"effective","granted_at":chrono::Utc::now(),"revoked_at":null,"binding_revision":f.binding_revision+1,"scope":{"format":"business_application_scope/2","origin":"https://example.test","device_id":f.device,"sender_account_id":f.sender,"schools":[f.school],"app_key":"wecom","capability":"group_text"}});
    let stale=f.repo.execute(f.principal.clone(),scope.clone(),"stale-setup-consent",command(json!({"operation":"dispatch","input":{"action":"authorization","receipt":receipt}})),true,"https://example.test").await;
    assert!(matches!(
        stale,
        Err(Error::Policy("authorization_context_changed"))
    ));
    receipt["binding_revision"] = json!(f.binding_revision);
    f.device_call(
        None,
        json!({"operation":"dispatch","input":{"action":"authorization","receipt":receipt}}),
    )
    .await;
    sqlx::query("DELETE FROM device_account_bindings WHERE device_id=$1")
        .bind(f.device)
        .execute(&f.pool)
        .await
        .unwrap();
    let status = f
        .repo
        .query(
            f.principal.clone(),
            scope,
            Query::Dispatch(DispatchQuery::SetupStatus),
            Page::default(),
        )
        .await
        .unwrap();
    assert!(status["sender_account_id"].is_null());
    assert_eq!(status["schools"], json!([]));
    receipt["state"] = json!("revoked");
    receipt["revision"] = json!(2);
    receipt["revoked_at"] = json!(chrono::Utc::now());
    receipt.as_object_mut().unwrap().remove("binding_revision");
    f.device_call(
        None,
        json!({"operation":"dispatch","input":{"action":"authorization","receipt":receipt}}),
    )
    .await;
    let body:Value=sqlx::query_scalar("SELECT body FROM automation_business_authorizations WHERE device_id=$1 ORDER BY revision DESC LIMIT 1").bind(f.device).fetch_one(&f.pool).await.unwrap();
    assert_eq!(body["state"], "revoked");
}
