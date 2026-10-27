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
    assert_eq!(doc["data"]["attributes"]["enabled"], false);
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
