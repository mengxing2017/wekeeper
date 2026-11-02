use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;
use wekeeper_api::{
    business::{Actor, Page, Result},
    database,
    distribution::{
        domain::broadcast::*,
        north::broadcast::Service,
        south::{
            adapter::postgres::broadcast::Repository,
            port::broadcast::{Client, Command, Store},
        },
    },
};
#[derive(Clone)]
struct Fake {
    calls: Arc<AtomicUsize>,
    unknown: bool,
    corp: String,
}
impl Client for Fake {
    async fn generation(&self) -> Result<(String, i64)> {
        Ok((self.corp.clone(), 1))
    }
    async fn verify(&self, _: &Work) -> Result<()> {
        Ok(())
    }
    async fn invoke(&self, w: &Work) -> Outcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.unknown {
            Outcome::Unknown
        } else if w.operation == "create" {
            Outcome::Created {
                msgid: format!("msg-{}", w.id),
                failed: vec![],
            }
        } else {
            Outcome::Done
        }
    }
    async fn results(&self, w: &Work) -> Result<Results> {
        Ok(Results {
            member_status: Some(2),
            groups: vec![Observation {
                chat_id: w.chats[0].clone(),
                status: 1,
                send_time: Some(1700000000),
            }],
        })
    }
    async fn candidates(&self, _: i64, _: i64) -> Result<Vec<Value>> {
        Ok(vec![])
    }
}
async fn setup() -> (PgPool, Actor, Uuid, DraftInput, Fake) {
    setup_with_forum(true).await
}
async fn setup_with_forum(with_forum: bool) -> (PgPool, Actor, Uuid, DraftInput, Fake) {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("broadcast_{}", Uuid::new_v4().simple());
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
    let s = Uuid::new_v4();
    let member = Uuid::new_v4();
    let version = Uuid::new_v4();
    let template = Uuid::new_v4();
    let post = Uuid::new_v4();
    let corp = format!("corp-{s}");
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'School')")
        .bind(s)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(true,$1,'scope')").bind(&corp).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'owner','Owner',1,'verified',now())").bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,username,display_name,wecom_member_id,platform_admin) VALUES($1,'admin','Admin',$2,true)").bind(actor).bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO school_memberships VALUES($1,$2,'admin')")
        .bind(s)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO templates(id,school_id,name) VALUES($1,$2,'Notice')")
        .bind(template)
        .bind(s)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO template_versions(id,school_id,template_id,body,published) VALUES($1,$2,$3,'{{post.title}}',true)").bind(version).bind(s).bind(template).execute(&pool).await.unwrap();
    if with_forum {
        sqlx::query("INSERT INTO posts(id,school_id,author_id,title,content,audit_status) VALUES($1,$2,$3,'Notice','Body','approved')").bind(post).bind(s).bind(actor).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO official_channel_deliveries(id,corp_id,generation,credential_scope,state,checked_at,valid_until,source) VALUES($1,$2,1,'scope','available',now(),now()+interval '1 hour','isolated-test-fixture')").bind(Uuid::new_v4()).bind(&corp).execute(&pool).await.unwrap();
    let mut groups = Vec::new();
    for i in 0..2 {
        let group = Uuid::new_v4();
        let directory = Uuid::new_v4();
        let chat = format!("chat-{i}");
        sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups',$2,$3,$4,now(),1,'visible','verified')").bind(directory).bind(&corp).bind(&chat).bind(json!({"owner":"owner"})).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$4,$5)").bind(group).bind(s).bind(&corp).bind(chat).bind(directory).execute(&pool).await.unwrap();
        groups.push(group);
    }
    let a:Actor=serde_json::from_value(json!({"id":actor,"username":"admin","display_name":"Admin","platform_admin":true,"bound_school_id":null,"grants":[{"school_id":s,"role":"admin"}],"wecom_authenticated":false})).unwrap();
    let input = DraftInput {
        request_key: None,
        materials: vec![],
        template_version_id: Some(version),
        content_source: ContentSource::Posts,
        text_content: None,
        post_ids: vec![post],
        group_ids: groups,
        revision: None,
        source_task_id: None,
        reason: String::new(),
    };
    (
        pool,
        a,
        s,
        input,
        Fake {
            calls: Arc::new(AtomicUsize::new(0)),
            unknown: false,
            corp,
        },
    )
}
fn action(revision: Option<i64>) -> Action {
    Action {
        preview_digest: None,
        revision,
        batch_id: None,
        decision: None,
        msgid: None,
        evidence_id: None,
        reason: String::new(),
    }
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn broadcast_once_partial_results_and_school_boundaries() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    assert_eq!(t.total, 2);
    assert_eq!(t.batch_count, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(service.task(&a, Uuid::new_v4(), t.id).await.is_err());
    let mut confirm = action(Some(t.revision));
    confirm.preview_digest = Some(service.preview(&a, s, t.id).await.unwrap().digest);
    service
        .action(&a, s, t.id, "confirm", "key", confirm.clone())
        .await
        .unwrap();
    service
        .action(&a, s, t.id, "confirm", "key", confirm)
        .await
        .unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                t.id,
                "confirm",
                "key",
                with_preview(&service, &a, s, t.id, action(Some(999))).await
            )
            .await
            .is_err()
    );
    let (x, y) = tokio::join!(service.run_once(), service.run_once());
    x.unwrap();
    y.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    service.run_once().await.unwrap();
    let t = service.task(&a, s, t.id).await.unwrap();
    assert_eq!(t.sent, 1);
    assert_eq!(t.pending, 1, "missing group is not fabricated as sent");
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().any(|r| r["official_status"].is_null()));
    let duplicate = service.save(&a, s, None, input.clone()).await.unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                duplicate.id,
                "confirm",
                "duplicate",
                with_preview(&service, &a, s, duplicate.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    // Successes are excluded by default in the UI, but an administrator can explicitly
    // select an original successful target with a reason after the old task is stopped.
    let mut explicit_repeat = input.clone();
    explicit_repeat.source_task_id = Some(t.id);
    explicit_repeat.reason = "Explicit repeat of a previously sent notice".into();
    explicit_repeat.group_ids = rows
        .iter()
        .filter(|r| r["official_status"] == 1)
        .map(|r| r["group_id"].as_str().unwrap().parse().unwrap())
        .collect();
    service.save(&a, s, None, explicit_repeat).await.unwrap();
    let mut retry = input;
    retry.source_task_id = Some(t.id);
    retry.reason = "failed targets only".into();
    retry.group_ids = rows
        .iter()
        .filter(|r| r["official_status"] != 1)
        .map(|r| r["group_id"].as_str().unwrap().parse().unwrap())
        .collect();
    let retry = service.save(&a, s, None, retry).await.unwrap();
    service
        .action(
            &a,
            s,
            retry.id,
            "confirm",
            "retry",
            with_preview(&service, &a, s, retry.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    // Platform administration alone is never a school sending grant.
    sqlx::query("DELETE FROM school_memberships WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.list(&a, s, Page::default()).await.is_err());
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn lost_response_and_crash_never_requeue_a_create() {
    let (pool, a, s, input, mut fake) = setup().await;
    fake.unknown = true;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    service
        .action(
            &a,
            s,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, s, t.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
    let duplicate = service.save(&a, s, None, input).await.unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                duplicate.id,
                "confirm",
                "dup",
                with_preview(&service, &a, s, duplicate.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    assert!(!service.store.claim(&w).await.unwrap());
    // Evidence is independent of platform state, and unlocks only explicitly confirmed non-creation.
    let mut reconcile = action(None);
    reconcile.batch_id = Some(w.id);
    reconcile.decision = Some("not_created".into());
    reconcile.reason = "checked enterprise admin logs".into();
    service
        .action(&a, s, t.id, "reconcile", "evidence", reconcile)
        .await
        .unwrap();
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // Simulate a process dying after the persisted invocation claim.
    sqlx::query("UPDATE broadcast_batches SET state='submitting',reconciliation=NULL,claimed_at=now()-interval '3 minutes' WHERE id=$1").bind(w.id).execute(&pool).await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, s, t.id).await.unwrap().uncertain, 1);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn drafts_are_revisioned_and_preflight_rechecks_content() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    let mut edit = input.clone();
    edit.revision = Some(1);
    let updated = service.save(&a, s, Some(t.id), edit.clone()).await.unwrap();
    assert_eq!(updated.revision, 2);
    assert!(service.save(&a, s, Some(t.id), edit).await.is_err());
    sqlx::query("UPDATE posts SET revision=revision+1 WHERE id=$1")
        .bind(input.post_ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                t.id,
                "confirm",
                "c",
                with_preview(&service, &a, s, t.id, action(Some(2))).await
            )
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    // Disabled deployments cannot enqueue an external creation even with valid local content.
    let disabled = Service {
        enabled: false,
        ..service.clone()
    };
    let fresh = disabled.save(&a, s, None, input).await.unwrap();
    assert!(
        disabled
            .action(
                &a,
                s,
                fresh.id,
                "confirm",
                "c",
                with_preview(&service, &a, s, fresh.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    assert_eq!(
        service.store.list(s, Page::default()).await.unwrap().len(),
        2
    );
    // Exercise command port without any HTTP-layer SQL.
    service
        .store
        .execute(
            &a,
            s,
            Command::Action {
                id: fresh.id,
                operation: "abandon".into(),
                key: "abandon".into(),
                input: action(None),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn stopping_during_creation_covers_late_created_task() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    service
        .action(
            &a,
            s,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, s, t.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    assert!(service.store.claim(&w).await.unwrap());
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    service
        .store
        .finish(
            &w,
            Outcome::Created {
                msgid: "late-msgid".into(),
                failed: vec![],
            },
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        1,
        "only the late stop is invoked"
    );
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn repeat_startup_preserves_current_draft_and_input() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let task = service.save(&a, s, None, input).await.unwrap();
    database::migrate(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    let restored = service.task(&a, s, task.id).await.unwrap();
    assert_eq!(restored.content, task.content);
    assert_eq!(restored.revision, task.revision);
    assert_eq!(restored.draft_input.group_ids, task.draft_input.group_ids);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn http_scope_csrf_retirement_and_real_task_contract() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    let mut application = wekeeper_api::bootstrap::build_application(pool.clone());
    application.options.public_origin = "https://school.test".into();
    application.options.insecure_local_auth = true;
    let (token, actor) = application
        .identity
        .0
        .session(a.id, "browser", false)
        .await
        .unwrap();
    let app = wekeeper_api::http::app_router(application);
    let path = format!("/api/v1/schools/{s}/broadcast-tasks/{}", t.id);
    let request = |method: &str, path: &str, csrf: bool, body: Value| {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "https://school.test")
            .header("content-type", "application/vnd.api+json")
            .header("idempotency-key", "http-confirm");
        if csrf {
            builder = builder.header("x-csrf-token", &actor.csrf);
        }
        builder
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(body.to_string())
            })
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(request("GET", &path, false, Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(value["data"]["type"], "broadcast-tasks");
    assert_eq!(value["data"]["attributes"]["total"], 2);
    let body = json!({"data":{"type":"broadcast-tasks","attributes":{"revision":1}}});
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("{path}/confirm"),
                false,
                body.clone()
            ))
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        app.clone()
            .oneshot(request("POST", &format!("{path}/confirm"), true, body))
            .await
            .unwrap()
            .status(),
        422,
        "missing server credentials or disabled broadcast must not invoke Tencent"
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/schools/{s}/delivery-drafts"),
                true,
                json!({"data":{"type":"delivery-drafts","attributes":{}}})
            ))
            .await
            .unwrap()
            .status(),
        404
    );
    sqlx::query("DELETE FROM school_memberships WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &path, false, Value::Null))
            .await
            .unwrap()
            .status(),
        403,
        "platform privilege does not bypass school grant"
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn reminders_are_limited_and_stop_unknown_requires_evidence() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let t = service.save(&a, s, None, input).await.unwrap();
    service
        .action(
            &a,
            s,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, s, t.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    service.store.claim(&w).await.unwrap();
    service
        .store
        .finish(
            &w,
            Outcome::Created {
                msgid: "reminder-test".into(),
                failed: vec![],
            },
        )
        .await
        .unwrap();
    for n in 0..3 {
        service
            .action(&a, s, t.id, "remind", &format!("remind-{n}"), action(None))
            .await
            .unwrap();
        let op = service
            .store
            .work(None)
            .await
            .unwrap()
            .into_iter()
            .find(|w| w.operation == "remind")
            .unwrap();
        assert!(service.store.claim(&op).await.unwrap());
        service.store.finish(&op, Outcome::Done).await.unwrap();
    }
    assert!(
        service
            .action(&a, s, t.id, "remind", "fourth", action(None))
            .await
            .is_err()
    );
    service
        .action(&a, s, t.id, "stop", "stop", action(None))
        .await
        .unwrap();
    let op = service
        .store
        .work(None)
        .await
        .unwrap()
        .into_iter()
        .find(|w| w.operation == "stop")
        .unwrap();
    assert!(service.store.claim(&op).await.unwrap());
    service.store.finish(&op, Outcome::Unknown).await.unwrap();
    assert!(
        !service
            .store
            .work(None)
            .await
            .unwrap()
            .iter()
            .any(|w| w.operation == "stop")
    );
    assert!(
        service
            .action(&a, s, t.id, "remind", "stopped-reminder", action(None))
            .await
            .is_err()
    );
    let mut evidence = action(None);
    evidence.batch_id = Some(w.id);
    evidence.decision = Some("stop_confirmed".into());
    evidence.reason = "checked official task stopped".into();
    service
        .action(&a, s, t.id, "reconcile", "evidence", evidence)
        .await
        .unwrap();
    let rows = service
        .rows(&a, s, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_text_without_posts_admin_confirmation_and_revocation() {
    let (pool, a, s, mut input, fake) = setup().await;
    sqlx::query("DELETE FROM posts")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM template_versions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM templates")
        .execute(&pool)
        .await
        .unwrap();
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    input.content_source = ContentSource::Text;
    input.template_version_id = None;
    input.post_ids.clear();
    input.text_content = Some("直接通知\r\n第二行😀".into());
    sqlx::query("UPDATE school_memberships SET role='operator' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    let t = service.save(&a, s, None, input.clone()).await.unwrap();
    assert_eq!(t.content, "直接通知\n第二行😀");
    assert_eq!(t.content_source, ContentSource::Text);
    assert_eq!(t.template_version_id, None);
    assert!(t.post_ids.is_empty());
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(
        service
            .action(
                &a,
                s,
                t.id,
                "confirm",
                "operator",
                with_preview(&service, &a, s, t.id, action(Some(t.revision))).await
            )
            .await
            .is_err()
    );
    // The repository enforces the same rule, not just the HTTP/service boundary.
    assert!(
        service
            .store
            .execute(
                &a,
                s,
                Command::Action {
                    id: t.id,
                    operation: "confirm".into(),
                    key: "bypass".into(),
                    input: action(Some(t.revision))
                }
            )
            .await
            .is_err()
    );
    sqlx::query("UPDATE school_memberships SET role='admin' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    service
        .action(
            &a,
            s,
            t.id,
            "confirm",
            "admin",
            with_preview(&service, &a, s, t.id, action(Some(t.revision))).await,
        )
        .await
        .unwrap();
    let work = service.store.work(Some(t.id)).await.unwrap();
    assert_eq!(work[0].content, t.content);
    sqlx::query("UPDATE school_memberships SET role='operator' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.store.claim(&work[0]).await.is_err());
    sqlx::query("UPDATE school_memberships SET role='admin' WHERE account_id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert!(service.save(&a, s, Some(t.id), input).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn content_source_switch_and_cross_source_duplicate_guard() {
    let (pool, a, s, posts, fake) = setup().await;
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake,
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let first = service.save(&a, s, None, posts.clone()).await.unwrap();
    let mut text = posts.clone();
    text.content_source = ContentSource::Text;
    text.template_version_id = None;
    text.post_ids.clear();
    text.text_content = Some(first.content.clone());
    text.revision = Some(first.revision);
    let changed = service
        .save(&a, s, Some(first.id), text.clone())
        .await
        .unwrap();
    assert_eq!(changed.revision, first.revision + 1);
    assert!(changed.post_ids.is_empty());
    assert!(
        service
            .action(
                &a,
                s,
                changed.id,
                "confirm",
                "stale",
                with_preview(&service, &a, s, changed.id, action(Some(first.revision))).await
            )
            .await
            .is_err()
    );
    let mut back = posts.clone();
    back.revision = Some(changed.revision);
    let restored = service.save(&a, s, Some(first.id), back).await.unwrap();
    assert_eq!(restored.content_source, ContentSource::Posts);
    assert_eq!(restored.post_ids, posts.post_ids);
    service
        .action(
            &a,
            s,
            restored.id,
            "confirm",
            "posts",
            with_preview(
                &service,
                &a,
                s,
                restored.id,
                action(Some(restored.revision)),
            )
            .await,
        )
        .await
        .unwrap();
    text.revision = None;
    let duplicate = service.save(&a, s, None, text).await.unwrap();
    assert!(
        service
            .action(
                &a,
                s,
                duplicate.id,
                "confirm",
                "text",
                with_preview(
                    &service,
                    &a,
                    s,
                    duplicate.id,
                    action(Some(duplicate.revision))
                )
                .await
            )
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn official_broadcast_ignores_mobile_switch_and_mobile_transaction_lock() {
    let (pool, a, s, input, fake) = setup().await;
    sqlx::query("INSERT INTO mobile_delivery_settings(school_id,enabled,history_reviewed) VALUES($1,true,true)")
        .bind(s).execute(&pool).await.unwrap();
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    // A mobile transaction must neither block official writes nor receive their results.
    let mut mobile_tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610241)")
        .execute(&mut *mobile_tx)
        .await
        .unwrap();
    let task = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service.save(&a, s, None, input),
    )
    .await
    .expect("official write waited on mobile lock")
    .unwrap();
    service
        .action(
            &a,
            s,
            task.id,
            "confirm",
            "isolated",
            with_preview(&service, &a, s, task.id, action(Some(task.revision))).await,
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM delivery_usage")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM broadcast_targets WHERE task_id=$1")
            .bind(task.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    mobile_tx.rollback().await.unwrap();
}

async fn with_preview<S: wekeeper_api::distribution::south::port::content_source::ContentSource>(
    service: &Service<Repository, Fake, S>,
    actor: &Actor,
    school: Uuid,
    id: Uuid,
    mut input: Action,
) -> Action {
    if let Ok(preview) = service.preview(actor, school, id).await {
        input.preview_digest = Some(preview.digest);
    }
    input
}

#[derive(Clone)]
struct NoticeSource;
impl wekeeper_api::distribution::south::port::content_source::ContentSource for NoticeSource {
    async fn check(
        &self,
        _: Uuid,
        _: &[wekeeper_api::distribution::domain::content::SourceReference],
    ) -> Result<()> {
        Ok(())
    }
    async fn read(
        &self,
        _: &Actor,
        _: Uuid,
        refs: &[wekeeper_api::distribution::domain::content::SourceReference],
    ) -> Result<Vec<wekeeper_api::distribution::domain::content::Material>> {
        Ok(refs
            .iter()
            .map(|r| wekeeper_api::distribution::domain::content::Material {
                reference: r.clone(),
                title: "独立通知".into(),
                content: format!("来源 {} 的通知", r.namespace),
                detail_url: String::new(),
            })
            .collect())
    }
}
async fn notice(
    pool: &PgPool,
    s: Uuid,
    namespace: &str,
) -> wekeeper_api::distribution::domain::content::SourceReference {
    sqlx::query("INSERT INTO distribution_sources(namespace) VALUES($1)")
        .bind(namespace)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO distribution_source_organizations VALUES($1,'1',$2,true)")
        .bind(namespace)
        .bind(s)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO distribution_materials VALUES($1,'1','1',$2,'v1','独立通知',$3,true,now())",
    )
    .bind(namespace)
    .bind(s)
    .bind(format!("来源 {namespace} 的通知"))
    .execute(pool)
    .await
    .unwrap();
    wekeeper_api::distribution::domain::content::SourceReference {
        namespace: namespace.into(),
        external_organization_id: "1".into(),
        external_content_id: "1".into(),
        version: "v1".into(),
    }
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn non_forum_source_replacement_idempotency_and_frozen_confirmation() {
    let (pool, a, s, mut input, fake) = setup_with_forum(false).await;
    let reference = notice(&pool, s, "test.notices").await;
    input.content_source = ContentSource::Materials;
    input.template_version_id = None;
    input.post_ids.clear();
    input.materials = vec![reference.clone()];
    input.request_key = Some("one-draft".into());
    let service = Service {
        store: Repository(pool.clone()),
        sources: NoticeSource,
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: String::new(),
    };
    let task = service.save(&a, s, None, input.clone()).await.unwrap();
    assert_eq!(
        task.id,
        service.save(&a, s, None, input.clone()).await.unwrap().id
    );
    assert_eq!(task.materials, vec![reference]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM posts")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let mut changed = input.clone();
    changed.group_ids.pop();
    assert!(service.save(&a, s, None, changed).await.is_err());
    assert!(
        service
            .action(&a, s, task.id, "confirm", "no-preview", action(Some(1)))
            .await
            .is_err()
    );
    let preview = service.preview(&a, s, task.id).await.unwrap();
    assert_eq!(preview.content, task.content);
    let mut confirm = action(Some(task.revision));
    confirm.preview_digest = Some(preview.digest);
    service
        .action(&a, s, task.id, "confirm", "confirmed", confirm.clone())
        .await
        .unwrap();
    service
        .action(&a, s, task.id, "confirm", "confirmed", confirm)
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    sqlx::query("DELETE FROM distribution_materials WHERE namespace='test.notices'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        service.task(&a, s, task.id).await.unwrap().content,
        task.content
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn two_sources_share_official_resource_budget_and_unknown_does_not_release() {
    let (pool, a, s, mut input, mut fake) = setup_with_forum(false).await;
    fake.unknown = true;
    let first = notice(&pool, s, "test.one").await;
    let second = notice(&pool, s, "test.two").await;
    input.content_source = ContentSource::Materials;
    input.template_version_id = None;
    input.post_ids.clear();
    let service = Service {
        store: Repository(pool.clone()),
        sources: NoticeSource,
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: String::new(),
    };
    let mut tasks = Vec::new();
    for reference in [first, second] {
        input.materials = vec![reference];
        let t = service.save(&a, s, None, input.clone()).await.unwrap();
        let body = with_preview(&service, &a, s, t.id, action(Some(t.revision))).await;
        service
            .action(&a, s, t.id, "confirm", &format!("confirm-{}", t.id), body)
            .await
            .unwrap();
        tasks.push(t);
    }
    sqlx::query("INSERT INTO distribution_resource_policies(resource_key,window_seconds,max_calls) VALUES($1,3600,1)").bind(format!("wecom:{}:owner",fake.corp)).execute(&pool).await.unwrap();
    let w1 = service
        .store
        .work(Some(tasks[0].id))
        .await
        .unwrap()
        .remove(0);
    let w2 = service
        .store
        .work(Some(tasks[1].id))
        .await
        .unwrap()
        .remove(0);
    let (r1, r2) = tokio::join!(service.store.claim(&w1), service.store.claim(&w2));
    assert_ne!(r1.unwrap(), r2.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM distribution_resource_usage")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let state: String = sqlx::query_scalar("SELECT state FROM broadcast_batches WHERE id=$1")
        .bind(w1.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let claimed = if state == "submitting" { w1 } else { w2 };
    service
        .store
        .finish(&claimed, Outcome::Unknown)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM distribution_resource_usage")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn forum_withdrawal_is_transactional_and_preserves_snapshot_and_history() {
    let (pool, a, s, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let task = service.save(&a, s, None, input.clone()).await.unwrap();
    let body = with_preview(&service, &a, s, task.id, action(Some(1))).await;
    service
        .action(&a, s, task.id, "confirm", "confirm", body)
        .await
        .unwrap();
    sqlx::query("UPDATE posts SET visibility='deleted',revision=revision+1 WHERE id=$1")
        .bind(input.post_ids[0])
        .execute(&pool)
        .await
        .unwrap();
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM distribution_source_events WHERE processed_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pending > 0);
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        service.task(&a, s, task.id).await.unwrap().content,
        task.content
    );
    assert!(
        sqlx::query("UPDATE distribution_message_snapshots SET content='changed'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM broadcast_batches WHERE state='cancelled'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn sender_qualification_is_platform_only_and_blocks_final_call() {
    let (pool, mut a, s, input, fake) = setup().await;
    let senders =
        wekeeper_api::distribution::south::adapter::postgres::senders::Repository(pool.clone());
    let sender = senders
        .list(&a, Some(s), Page::default())
        .await
        .unwrap()
        .remove(0);
    let service = Service {
        store: Repository(pool.clone()),
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    };
    let task = service.save(&a, s, None, input).await.unwrap();
    let body = with_preview(&service, &a, s, task.id, action(Some(1))).await;
    service
        .action(&a, s, task.id, "confirm", "confirm", body)
        .await
        .unwrap();
    use wekeeper_api::distribution::domain::content::SenderQualificationInput;
    a.platform_admin = false;
    assert!(
        senders
            .qualify(
                &a,
                s,
                sender.id,
                SenderQualificationInput {
                    enabled: false,
                    revision: 0
                }
            )
            .await
            .is_err()
    );
    a.platform_admin = true;
    senders
        .qualify(
            &a,
            s,
            sender.id,
            SenderQualificationInput {
                enabled: false,
                revision: 0,
            },
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn product_unavailable_draft_replays_without_sender_or_external_work() {
    let (pool, a, s, mut input, fake) = setup().await;
    input.content_source = ContentSource::Text;
    input.text_content = Some("待开通通知".into());
    input.template_version_id = None;
    input.post_ids.clear();
    input.group_ids.clear();
    let service = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: false,
        management_available: false,
        origin: "https://school.test".into(),
    };
    let first = service
        .save_preview(&a, s, None, input.clone(), "draft-preview")
        .await
        .unwrap();
    assert!(first["preview"].is_null());
    assert_eq!(first["saved"], true);
    assert_eq!(
        first,
        service
            .save_preview(&a, s, None, input.clone(), "draft-preview")
            .await
            .unwrap()
    );
    input.text_content = Some("不同通知".into());
    assert!(
        service
            .save_preview(&a, s, None, input, "draft-preview")
            .await
            .is_err()
    );
    let id = first["task_id"].as_str().unwrap().parse().unwrap();
    let task = service.task(&a, s, id).await.unwrap();
    assert_eq!(task.total, 0);
    assert_eq!(task.state, "draft");
    let scope: Option<String> =
        sqlx::query_scalar("SELECT corp_id FROM broadcast_tasks WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(scope.is_none());
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn product_template_confirmation_is_one_time_and_draft_preserves_published_body() {
    use wekeeper_api::distribution::{
        domain::workflows::{Command as C, Confirmation, TemplateInput},
        north::local::workflows::Service as W,
        south::adapter::postgres::workflows::Repository as R,
    };
    let (pool, a, s, _, _) = setup().await;
    let service = W(R(pool.clone()));
    let input = TemplateInput {
        id: None,
        revision: 0,
        name: "两步通知".into(),
        body: "{{school.name}} 通知".into(),
        note: "首次发布".into(),
        sample_post_id: None,
        publish: true,
    };
    let preview = service
        .command(&a, Some(s), C::TemplatePreview(input.clone()))
        .await
        .unwrap();
    assert!(preview.get("dependencies").is_none());
    let c = Confirmation {
        preview_id: preview["id"].as_str().unwrap().parse().unwrap(),
        digest: preview["digest"].as_str().unwrap().into(),
    };
    let (one, two) = tokio::join!(
        service.command(&a, Some(s), C::TemplateConfirm(c.clone())),
        service.command(&a, Some(s), C::TemplateConfirm(c.clone()))
    );
    let one = one.unwrap();
    assert_eq!(one, two.unwrap());
    let id: Uuid = one["id"].as_str().unwrap().parse().unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM template_versions WHERE template_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let input = TemplateInput {
        id: Some(id),
        revision: 1,
        body: "未发布草稿".into(),
        publish: false,
        ..input
    };
    let p = service
        .command(&a, Some(s), C::TemplatePreview(input))
        .await
        .unwrap();
    let result = service
        .command(
            &a,
            Some(s),
            C::TemplateConfirm(Confirmation {
                preview_id: p["id"].as_str().unwrap().parse().unwrap(),
                digest: p["digest"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(result["published"], false);
    let body:String=sqlx::query_scalar("SELECT v.body FROM templates t JOIN template_versions v ON v.id=t.current_version_id WHERE t.id=$1").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(body, "{{school.name}} 通知");
    assert!(
        service
            .command(&a, Some(Uuid::new_v4()), C::TemplateConfirm(c))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn product_private_media_and_expired_school_override_keep_scope() {
    use wekeeper_api::schools::domain::{ImageRef, ResourceAction, ResourceInput};
    use wekeeper_api::schools::{
        north::resources::Service as M,
        south::{adapter::postgres::Repository as R, port::resources::Command as C},
    };
    let (pool, a, s, _, _) = setup().await;
    let service = M(R(pool.clone()));
    let art = service
        .qr(&a, None, "https://school.test/help")
        .await
        .unwrap();
    let asset: Uuid = art["id"].as_str().unwrap().parse().unwrap();
    let input = ResourceInput {
        id: None,
        revision: 0,
        kind: "banner".into(),
        title: "平台默认".into(),
        content: "说明".into(),
        image_ref: Some(ImageRef::Asset { id: asset }),
        action: ResourceAction::None {},
        active: true,
        sort_order: 0,
        starts_at: None,
        ends_at: None,
    };
    assert!(
        service
            .command(&a, Some(s), C::Preview(input.clone()))
            .await
            .is_err(),
        "school cannot steal a platform draft"
    );
    let p = service.command(&a, None, C::Preview(input)).await.unwrap();
    let command = C::Confirm {
        preview_id: p["id"].as_str().unwrap().parse().unwrap(),
        digest: p["digest"].as_str().unwrap().into(),
    };
    let result = service.command(&a, None, command.clone()).await.unwrap();
    assert_eq!(result, service.command(&a, None, command).await.unwrap());
    let id: Uuid = result["id"].as_str().unwrap().parse().unwrap();
    assert!(service.media(&a, Some(s), asset, false).await.is_err());
    assert!(service.media(&a, Some(s), id, true).await.is_ok());
    sqlx::query("INSERT INTO school_resources(id,school_id,kind,title,active,ends_at) VALUES($1,$2,'banner','过期覆盖',true,now()-interval '1 second')").bind(Uuid::new_v4()).bind(s).execute(&pool).await.unwrap();
    let list = R(pool.clone()).resources(s, false).await.unwrap();
    assert!(
        list.iter().any(|v| v["id"] == json!(id)),
        "expired override falls back to platform"
    );
    sqlx::query("UPDATE school_resources SET active=false WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.media(&a, Some(s), id, true).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn product_member_policy_reuses_existing_key_and_usage_ledger() {
    use wekeeper_api::distribution::{
        domain::workflows::{Command as C, Confirmation, MemberPolicyInput},
        north::local::workflows::Service as W,
        south::adapter::postgres::workflows::Repository as R,
    };
    let (pool, a, s, input, fake) = setup().await;
    let service = W(R(pool.clone()));
    let member: Uuid = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1")
        .bind(a.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let draft = Service {
        sources: wekeeper_api::distribution::south::adapter::in_process::Sources(pool.clone()),
        store: Repository(pool.clone()),
        client: fake.clone(),
        enabled: true,
        management_available: true,
        origin: "https://school.test".into(),
    }
    .save(&a, s, None, input)
    .await
    .unwrap();
    let batch = Repository(pool.clone()).work(Some(draft.id)).await.unwrap()[0].id;
    let key = format!("wecom:{}:owner", fake.corp);
    sqlx::query("INSERT INTO distribution_resource_policies(resource_key,window_seconds,max_calls) VALUES($1,3600,3)").bind(&key).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO distribution_resource_usage(batch_id,resource_key,source_namespace) VALUES($1,$2,'builtin.forum')").bind(batch).bind(&key).execute(&pool).await.unwrap();
    // Existing ledger and unknown reservations belong to this key; updating policy must never delete them.
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM distribution_resource_policies p WHERE resource_key=$1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    let p = service
        .command(
            &a,
            None,
            C::PolicyPreview(MemberPolicyInput {
                member_id: member,
                generation: 1,
                revision: before["revision"].as_i64().unwrap(),
                enabled: false,
                window_seconds: 7200,
                max_calls: 2,
            }),
        )
        .await
        .unwrap();
    let c = Confirmation {
        preview_id: p["id"].as_str().unwrap().parse().unwrap(),
        digest: p["digest"].as_str().unwrap().into(),
    };
    let result = service
        .command(&a, None, C::PolicyConfirm(c.clone()))
        .await
        .unwrap();
    assert_eq!(
        result,
        service
            .command(&a, None, C::PolicyConfirm(c))
            .await
            .unwrap()
    );
    let after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM distribution_resource_policies p WHERE resource_key=$1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after["id"], before["id"]);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM distribution_resource_usage WHERE resource_key=$1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(after["enabled"], false);
    assert_eq!(after["window_seconds"], 7200);
    assert!(service.options(&a, Some(Uuid::new_v4())).await.is_err());
    assert!(
        service.channel(&a, s, true, true, "scope").await.unwrap()["broadcast_available"]
            .as_bool()
            .unwrap()
    );
    sqlx::query("DELETE FROM official_channel_deliveries")
        .execute(&pool)
        .await
        .unwrap();
    let unavailable = service.channel(&a, s, true, true, "scope").await.unwrap();
    assert_eq!(unavailable["broadcast_available"], false);
    assert_eq!(unavailable["delivery_state"], "awaiting_delivery");
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn product_usage_request_has_no_early_grant_and_approval_revalidates_directory() {
    use wekeeper_api::distribution::{
        domain::workflows::{Command as C, Confirmation, RequestDecision, UsageInput},
        north::local::workflows::Service as W,
        south::adapter::postgres::workflows::Repository as R,
    };
    let (pool, mut school_actor, s, _, _) = setup().await;
    let admin_id = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'approver','负责人',true)").bind(admin_id).execute(&pool).await.unwrap();
    let mut platform = school_actor.clone();
    platform.id = admin_id;
    sqlx::query("UPDATE accounts SET platform_admin=false WHERE id=$1")
        .bind(school_actor.id)
        .execute(&pool)
        .await
        .unwrap();
    school_actor.platform_admin = false;
    sqlx::query("INSERT INTO wecom_sender_qualifications(school_id,account_id,enabled,updated_by) VALUES($1,$2,false,$2)").bind(s).bind(school_actor.id).execute(&pool).await.unwrap();
    let member: Uuid = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1")
        .bind(school_actor.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let groups: Vec<Uuid> = sqlx::query_scalar(
        "SELECT directory_group_id FROM wecom_groups WHERE school_id=$1 ORDER BY id",
    )
    .bind(s)
    .fetch_all(&pool)
    .await
    .unwrap();
    let service = W(R(pool.clone()));
    let v = UsageInput {
        member_id: member,
        account_id: school_actor.id,
        generation: 1,
        role: "admin".into(),
        group_ids: groups.clone(),
    };
    let p = service
        .command(&school_actor, Some(s), C::UsagePreview(v))
        .await
        .unwrap();
    assert_eq!(p["requires_approval"], true);
    let c = Confirmation {
        preview_id: p["id"].as_str().unwrap().parse().unwrap(),
        digest: p["digest"].as_str().unwrap().into(),
    };
    let req = service
        .command(&school_actor, Some(s), C::UsageConfirm(c.clone()))
        .await
        .unwrap();
    assert!(req["payload"].get("snapshot").is_none());
    assert_eq!(req["state"], "pending");
    assert_eq!(
        req,
        service
            .command(&school_actor, Some(s), C::UsageConfirm(c))
            .await
            .unwrap()
    );
    let enabled: bool = sqlx::query_scalar(
        "SELECT enabled FROM wecom_sender_qualifications WHERE school_id=$1 AND account_id=$2",
    )
    .bind(s)
    .bind(school_actor.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!enabled);
    sqlx::query(
        "UPDATE wecom_asset_objects SET data=jsonb_set(data,'{owner}','\"different\"') WHERE id=$1",
    )
    .bind(groups[0])
    .execute(&pool)
    .await
    .unwrap();
    let id = req["id"].as_str().unwrap().parse().unwrap();
    assert!(
        service
            .command(
                &platform,
                None,
                C::Decision {
                    id,
                    input: RequestDecision {
                        revision: 1,
                        state: "approved".into(),
                        response: "已核对范围".into(),
                        owner_id: None
                    }
                }
            )
            .await
            .is_err()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM product_requests WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "pending");
    let enabled: bool = sqlx::query_scalar(
        "SELECT enabled FROM wecom_sender_qualifications WHERE school_id=$1 AND account_id=$2",
    )
    .bind(s)
    .bind(school_actor.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !enabled,
        "failed approval cannot leave partial authorization"
    );
}
