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
async fn setup() -> (PgPool, Actor, DraftInput, Fake) {
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
    let member = Uuid::new_v4();
    let corp = format!("corp-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(true,$1,'scope')").bind(&corp).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wecom_members(id,userid,name,status,verification,checked_at) VALUES($1,'owner','Owner',1,'verified',now())").bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,username,display_name,wecom_member_id,platform_admin) VALUES($1,'admin','Admin',$2,true)").bind(actor).bind(member).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO official_channel_health(corp_id,generation,credential_scope,available,checked_at,last_success_at) VALUES($1,1,'scope',true,now(),now())").bind(&corp).execute(&pool).await.unwrap();
    let mut groups = Vec::new();
    for i in 0..2 {
        let group = Uuid::new_v4();
        let directory = Uuid::new_v4();
        let chat = format!("chat-{i}");
        sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups',$2,$3,$4,now(),1,'visible','verified')").bind(directory).bind(&corp).bind(&chat).bind(json!({"owner":"owner"})).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO wecom_groups(id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$3,$4)").bind(group).bind(&corp).bind(chat).bind(directory).execute(&pool).await.unwrap();
        groups.push(group);
    }
    let a:Actor=serde_json::from_value(json!({"id":actor,"username":"admin","display_name":"Admin","platform_admin":true,"wecom_authenticated":false})).unwrap();
    let input = DraftInput {
        request_key: None,
        content_source: ContentSource::Text,
        text_content: Some("Notice".into()),
        group_ids: groups,
        revision: None,
        source_task_id: None,
        reason: String::new(),
    };
    (
        pool,
        a,
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
async fn broadcast_once_partial_results_and_administrator_boundaries() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let t = service.save(&a, None, input.clone()).await.unwrap();
    assert_eq!(t.total, 2);
    assert_eq!(t.batch_count, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(service.task(&a, Uuid::new_v4()).await.is_err());
    let mut confirm = action(Some(t.revision));
    confirm.preview_digest = Some(service.preview(&a, t.id).await.unwrap().digest);
    service
        .action(&a, t.id, "confirm", "key", confirm.clone())
        .await
        .unwrap();
    service
        .action(&a, t.id, "confirm", "key", confirm)
        .await
        .unwrap();
    assert!(
        service
            .action(
                &a,
                t.id,
                "confirm",
                "key",
                with_preview(&service, &a, t.id, action(Some(999))).await
            )
            .await
            .is_err()
    );
    let (x, y) = tokio::join!(service.run_once(), service.run_once());
    x.unwrap();
    y.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    service.run_once().await.unwrap();
    let t = service.task(&a, t.id).await.unwrap();
    assert_eq!(t.sent, 1);
    assert_eq!(t.pending, 1, "missing group is not fabricated as sent");
    let rows = service
        .rows(&a, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().any(|r| r["official_status"].is_null()));
    let duplicate = service.save(&a, None, input.clone()).await.unwrap();
    assert!(
        service
            .action(
                &a,
                duplicate.id,
                "confirm",
                "duplicate",
                with_preview(&service, &a, duplicate.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    service
        .action(&a, t.id, "stop", "stop", action(None))
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
    service.save(&a, None, explicit_repeat).await.unwrap();
    let mut retry = input;
    retry.source_task_id = Some(t.id);
    retry.reason = "failed targets only".into();
    retry.group_ids = rows
        .iter()
        .filter(|r| r["official_status"] != 1)
        .map(|r| r["group_id"].as_str().unwrap().parse().unwrap())
        .collect();
    let retry = service.save(&a, None, retry).await.unwrap();
    service
        .action(
            &a,
            retry.id,
            "confirm",
            "retry",
            with_preview(&service, &a, retry.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    // Global administrators can inspect all tasks; an enabled sender executes the retry.
    assert!(service.list(&a, Page::default()).await.is_ok());
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn lost_response_and_crash_never_requeue_a_create() {
    let (pool, a, input, mut fake) = setup().await;
    fake.unknown = true;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let t = service.save(&a, None, input.clone()).await.unwrap();
    service
        .action(
            &a,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, t.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, t.id).await.unwrap().uncertain, 1);
    let duplicate = service.save(&a, None, input).await.unwrap();
    assert!(
        service
            .action(
                &a,
                duplicate.id,
                "confirm",
                "dup",
                with_preview(&service, &a, duplicate.id, action(Some(1))).await
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
        .action(&a, t.id, "reconcile", "evidence", reconcile)
        .await
        .unwrap();
    assert_eq!(service.task(&a, t.id).await.unwrap().uncertain, 1);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // Simulate a process dying after the persisted invocation claim.
    sqlx::query("UPDATE broadcast_batches SET state='submitting',reconciliation=NULL,claimed_at=now()-interval '3 minutes' WHERE id=$1").bind(w.id).execute(&pool).await.unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.task(&a, t.id).await.unwrap().uncertain, 1);
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn drafts_are_revisioned_and_preflight_rechecks_content() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let t = service.save(&a, None, input.clone()).await.unwrap();
    let mut edit = input.clone();
    edit.revision = Some(1);
    let updated = service.save(&a, Some(t.id), edit.clone()).await.unwrap();
    assert_eq!(updated.revision, 2);
    assert!(service.save(&a, Some(t.id), edit).await.is_err());
    assert!(
        service
            .action(
                &a,
                t.id,
                "confirm",
                "c",
                with_preview(&service, &a, t.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    // Missing upstream credentials cannot enqueue an external creation even with valid local content.
    let unconfigured = Service {
        management_available: false,
        ..service.clone()
    };
    let fresh = unconfigured.save(&a, None, input).await.unwrap();
    assert!(
        unconfigured
            .action(
                &a,
                fresh.id,
                "confirm",
                "c",
                with_preview(&service, &a, fresh.id, action(Some(1))).await
            )
            .await
            .is_err()
    );
    assert_eq!(service.store.list(Page::default()).await.unwrap().len(), 2);
    // Exercise command port without any HTTP-layer SQL.
    service
        .store
        .execute(
            &a,
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
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool),
        client: fake.clone(),
        management_available: true,
    };
    let t = service.save(&a, None, input).await.unwrap();
    service
        .action(
            &a,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, t.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    let w = service.store.work(Some(t.id)).await.unwrap().remove(0);
    assert!(service.store.claim(&w).await.unwrap());
    service
        .action(&a, t.id, "stop", "stop", action(None))
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
        .rows(&a, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn repeat_startup_preserves_current_draft_and_input() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let task = service.save(&a, None, input).await.unwrap();
    database::migrate(&pool).await.unwrap();
    database::migrate(&pool).await.unwrap();
    let restored = service.task(&a, task.id).await.unwrap();
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
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake,
        management_available: true,
    };
    let t = service.save(&a, None, input).await.unwrap();
    let mut application = wekeeper_api::bootstrap::build_application(pool.clone());
    application.options.public_origin = "https://distribution.test".into();
    application.options.insecure_local_auth = true;
    let (token, actor) = application
        .identity
        .0
        .session(a.id, "browser", false)
        .await
        .unwrap();
    let app = wekeeper_api::http::app_router(application);
    let path = format!("/api/v1/wecom-distribution/broadcast-tasks/{}", t.id);
    let request = |method: &str, path: &str, csrf: bool, body: Value| {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "https://distribution.test")
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
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("{path}/observations"),
            false,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let observations: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert!(observations["data"][0]["id"].is_string());
    assert_eq!(
        observations["data"][0]["attributes"]["evidence_type"],
        "platform_accepted"
    );
    assert!(observations["data"][0]["attributes"].get("id").is_none());
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
            .oneshot(request("GET", &path, false, Value::Null))
            .await
            .unwrap()
            .status(),
        200,
        "platform privilege can inspect independent workspaces"
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn reminders_are_limited_and_stop_unknown_requires_evidence() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool),
        client: fake,
        management_available: true,
    };
    let t = service.save(&a, None, input).await.unwrap();
    service
        .action(
            &a,
            t.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, t.id, action(Some(1))).await,
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
            .action(&a, t.id, "remind", &format!("remind-{n}"), action(None))
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
            .action(&a, t.id, "remind", "fourth", action(None))
            .await
            .is_err()
    );
    service
        .action(&a, t.id, "stop", "stop", action(None))
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
            .action(&a, t.id, "remind", "stopped-reminder", action(None))
            .await
            .is_err()
    );
    let mut evidence = action(None);
    evidence.batch_id = Some(w.id);
    evidence.decision = Some("stop_confirmed".into());
    evidence.reason = "checked official task stopped".into();
    service
        .action(&a, t.id, "reconcile", "evidence", evidence)
        .await
        .unwrap();
    let rows = service
        .rows(&a, t.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(rows.iter().all(|r| r["stop_state"] == "stopped"));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_text_requires_current_administrator_confirmation() {
    let (pool, mut a, mut input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    input.content_source = ContentSource::Text;
    input.text_content = Some("直接通知\r\n第二行😀".into());
    let task = service.save(&a, None, input.clone()).await.unwrap();
    assert_eq!(task.content, "直接通知\n第二行😀");
    let confirmation = with_preview(&service, &a, task.id, action(Some(task.revision))).await;
    sqlx::query("UPDATE accounts SET platform_admin=false WHERE id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    a.platform_admin = false;
    assert!(
        service
            .action(&a, task.id, "confirm", "denied", confirmation.clone())
            .await
            .is_err()
    );
    assert!(
        service
            .store
            .execute(
                &a,
                Command::Action {
                    id: task.id,
                    operation: "confirm".into(),
                    key: "bypass".into(),
                    input: confirmation.clone()
                }
            )
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    sqlx::query("UPDATE accounts SET platform_admin=true WHERE id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    a.platform_admin = true;
    service
        .action(&a, task.id, "confirm", "admin", confirmation)
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert!(service.save(&a, Some(task.id), input).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn direct_text_duplicate_guard() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake,
        management_available: true,
    };
    let first = service.save(&a, None, input.clone()).await.unwrap();
    let mut text = input.clone();
    text.content_source = ContentSource::Text;
    text.text_content = Some(first.content.clone());
    text.revision = Some(first.revision);
    let changed = service
        .save(&a, Some(first.id), text.clone())
        .await
        .unwrap();
    assert_eq!(changed.revision, first.revision + 1);
    assert!(
        service
            .action(
                &a,
                changed.id,
                "confirm",
                "stale",
                with_preview(&service, &a, changed.id, action(Some(first.revision))).await
            )
            .await
            .is_err()
    );
    let mut back = input.clone();
    back.revision = Some(changed.revision);
    let restored = service.save(&a, Some(first.id), back).await.unwrap();
    assert_eq!(restored.content_source, ContentSource::Text);
    service
        .action(
            &a,
            restored.id,
            "confirm",
            "original",
            with_preview(&service, &a, restored.id, action(Some(restored.revision))).await,
        )
        .await
        .unwrap();
    text.revision = None;
    let duplicate = service.save(&a, None, text).await.unwrap();
    assert!(
        service
            .action(
                &a,
                duplicate.id,
                "confirm",
                "text",
                with_preview(&service, &a, duplicate.id, action(Some(duplicate.revision))).await
            )
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn official_broadcast_ignores_mobile_switch_and_mobile_transaction_lock() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    // A mobile transaction must neither block official writes nor receive their results.
    let mut mobile_tx = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(78234610245)")
        .execute(&mut *mobile_tx)
        .await
        .unwrap();
    let task = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service.save(&a, None, input),
    )
    .await
    .expect("official write waited on mobile lock")
    .unwrap();
    service
        .action(
            &a,
            task.id,
            "confirm",
            "isolated",
            with_preview(&service, &a, task.id, action(Some(task.revision))).await,
        )
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('delivery_usage') IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap()
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

async fn with_preview(
    service: &Service<Repository, Fake>,
    actor: &Actor,
    id: Uuid,
    mut input: Action,
) -> Action {
    if let Ok(preview) = service.preview(actor, id).await {
        input.preview_digest = Some(preview.digest);
    }
    input
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn confirmed_text_messages_keep_immutable_snapshots() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let task = service.save(&a, None, input.clone()).await.unwrap();
    let body = with_preview(&service, &a, task.id, action(Some(1))).await;
    service
        .action(&a, task.id, "confirm", "confirm", body)
        .await
        .unwrap();
    service.run_once().await.unwrap();
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        service.task(&a, task.id).await.unwrap().content,
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
        0
    );
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn sender_qualification_is_platform_only_and_blocks_final_call() {
    let (pool, mut a, input, fake) = setup().await;
    let senders =
        wekeeper_api::distribution::south::adapter::postgres::senders::Repository(pool.clone());
    let sender = senders.list(&a, Page::default()).await.unwrap().remove(0);
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let task = service.save(&a, None, input).await.unwrap();
    let body = with_preview(&service, &a, task.id, action(Some(1))).await;
    service
        .action(&a, task.id, "confirm", "confirm", body)
        .await
        .unwrap();
    use wekeeper_api::distribution::domain::content::SenderQualificationInput;
    a.platform_admin = false;
    assert!(
        senders
            .qualify(
                &a,
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
    let (pool, a, mut input, fake) = setup().await;
    input.content_source = ContentSource::Text;
    input.text_content = Some("待开通通知".into());
    input.group_ids.clear();
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: false,
    };
    let first = service
        .save_preview(&a, None, input.clone(), "draft-preview")
        .await
        .unwrap();
    assert!(first["preview"].is_null());
    assert_eq!(first["saved"], true);
    assert_eq!(
        first,
        service
            .save_preview(&a, None, input.clone(), "draft-preview")
            .await
            .unwrap()
    );
    input.text_content = Some("不同通知".into());
    assert!(
        service
            .save_preview(&a, None, input, "draft-preview")
            .await
            .is_err()
    );
    let id = first["task_id"].as_str().unwrap().parse().unwrap();
    let task = service.task(&a, id).await.unwrap();
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
async fn product_member_policy_reuses_existing_key_and_usage_ledger() {
    use wekeeper_api::distribution::{
        domain::workflows::{Command as C, Confirmation, MemberPolicyInput},
        north::local::workflows::Service as W,
        south::adapter::postgres::workflows::Repository as R,
    };
    let (pool, a, input, fake) = setup().await;
    let service = W(R(pool.clone()));
    let member: Uuid = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1")
        .bind(a.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let draft = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    }
    .save(&a, None, input)
    .await
    .unwrap();
    let batch = Repository(pool.clone()).work(Some(draft.id)).await.unwrap()[0].id;
    let key = format!("wecom:{}:owner", fake.corp);
    sqlx::query("INSERT INTO distribution_resource_policies(resource_key,window_seconds,max_calls) VALUES($1,3600,3)").bind(&key).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO distribution_resource_usage(batch_id,resource_key,source_namespace) VALUES($1,$2,'builtin.direct')").bind(batch).bind(&key).execute(&pool).await.unwrap();
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
        .command(&a, C::PolicyConfirm(c.clone()))
        .await
        .unwrap();
    assert_eq!(
        result,
        service.command(&a, C::PolicyConfirm(c)).await.unwrap()
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
    let options = service.options(&a).await.unwrap();
    let member_options = options["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == member.to_string())
        .unwrap();
    assert_eq!(member_options["policy"]["enabled"], false);
    assert_eq!(member_options["used"], 1);
    assert!(
        service.channel(&a, true, "scope").await.unwrap()["broadcast_available"]
            .as_bool()
            .unwrap()
    );
    sqlx::query("DELETE FROM official_channel_health")
        .execute(&pool)
        .await
        .unwrap();
    let unavailable = service.channel(&a, true, "scope").await.unwrap();
    assert_eq!(unavailable["broadcast_available"], false);
    assert_eq!(unavailable["health_state"], "checking");
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn automatic_health_never_unpauses_explicit_business_control() {
    use wekeeper_api::distribution::{
        domain::workflows::{ChannelControl, ChannelHealth},
        south::{adapter::postgres::workflows::Repository, port::workflows::Store},
    };
    let (pool, actor, _, _) = setup().await;
    let repo = Repository(pool.clone());
    let corp: String =
        sqlx::query_scalar("SELECT corp_id FROM wecom_directory_state WHERE singleton")
            .fetch_one(&pool)
            .await
            .unwrap();
    let control = repo
        .channel_control(
            &actor,
            "scope",
            ChannelControl {
                paused: true,
                expected_revision: 0,
                reason: "人工暂停".into(),
            },
        )
        .await
        .unwrap();
    let observation = ChannelHealth {
        scope: "scope".into(),
        corp: Some(corp),
        generation: Some(1),
        available: true,
        error_code: None,
    };
    repo.record_health(observation.clone()).await.unwrap();
    let paused = repo.channel(&actor, true, "scope").await.unwrap();
    assert_eq!(paused["broadcast_available"], false);
    assert_eq!(paused["health_state"], "paused");
    assert_eq!(paused["management_available"], true);
    assert!(paused.get("can_preview").is_none());
    repo.channel_control(
        &actor,
        "scope",
        ChannelControl {
            paused: false,
            expected_revision: control["revision"].as_i64().unwrap(),
            reason: "恢复新发送".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        repo.channel(&actor, true, "scope").await.unwrap()["broadcast_available"],
        true
    );
    repo.record_health(ChannelHealth {
        available: false,
        error_code: Some("wecom_48002".into()),
        ..observation
    })
    .await
    .unwrap();
    let failed = repo.channel(&actor, true, "scope").await.unwrap();
    assert_eq!(failed["broadcast_available"], false);
    let history: (bool, bool, Option<String>) = sqlx::query_as("SELECT last_success_at IS NOT NULL,last_failure_at IS NOT NULL,error_code FROM official_channel_health WHERE credential_scope='scope'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(history, (true, true, Some("wecom_48002".into())));
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn execution_evidence_preserves_creation_pending_success_and_read_backoff() {
    let (pool, a, input, fake) = setup().await;
    let service = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        management_available: true,
    };
    let task = service.save(&a, None, input).await.unwrap();
    service
        .action(
            &a,
            task.id,
            "confirm",
            "confirm",
            with_preview(&service, &a, task.id, action(Some(1))).await,
        )
        .await
        .unwrap();
    let mut work = service.store.work(Some(task.id)).await.unwrap().remove(0);
    let preflight = Work {
        operation: "preflight".into(),
        ..work.clone()
    };
    service
        .store
        .finish(&preflight, Outcome::Rejected("preflight_failed".into()))
        .await
        .unwrap();
    let state: (String, i32, Option<Uuid>) = sqlx::query_as(
        "SELECT state,sync_failures,invocation_id FROM broadcast_batches WHERE id=$1",
    )
    .bind(work.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("queued".into(), 1, None));
    assert!(service.store.claim(&work).await.unwrap());
    service
        .store
        .finish(
            &work,
            Outcome::Created {
                msgid: "evidence-message".into(),
                failed: vec![],
            },
        )
        .await
        .unwrap();
    let created = service.task(&a, task.id).await.unwrap();
    assert_eq!((created.sent, created.pending), (0, 2));
    let (digest, submitted): (Option<String>, bool) = sqlx::query_as(
        "SELECT request_digest,submitted_at IS NOT NULL FROM broadcast_batches WHERE id=$1",
    )
    .bind(work.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(submitted && digest.is_some_and(|v| v.len() == 64));
    work.msgid = Some("evidence-message".into());
    work.operation = "read".into();
    service
        .store
        .observed(
            &work,
            Ok(Results {
                member_status: Some(2),
                groups: vec![],
            }),
        )
        .await
        .unwrap();
    assert_eq!(service.task(&a, task.id).await.unwrap().sent, 0);
    let result = Results {
        member_status: Some(2),
        groups: vec![Observation {
            chat_id: work.chats[0].clone(),
            status: 1,
            send_time: Some(1700000000),
        }],
    };
    service
        .store
        .observed(&work, Ok(result.clone()))
        .await
        .unwrap();
    service.store.observed(&work, Ok(result)).await.unwrap();
    service
        .store
        .observed(&work, Err(wekeeper_api::business::Error::Unavailable))
        .await
        .unwrap();
    let backoff:(i32,bool)=sqlx::query_as("SELECT sync_failures,next_sync_at BETWEEN clock_timestamp()+interval '3 seconds' AND clock_timestamp()+interval '10 seconds' FROM broadcast_batches WHERE id=$1").bind(work.id).fetch_one(&pool).await.unwrap();
    assert_eq!(backoff, (1, true));
    assert_eq!(service.task(&a, task.id).await.unwrap().sent, 1);
    let rows = service
        .rows(&a, task.id, "observations", Page::default())
        .await
        .unwrap();
    assert!(
        rows.iter()
            .any(|v| v["evidence_type"] == "official_task_created"
                && v["normalized_result"] == "pending")
    );
    assert!(
        rows.iter()
            .any(|v| v["evidence_type"] == "external_member_executed"
                && v["normalized_result"] == "pending")
    );
    assert_eq!(
        rows.iter()
            .filter(|v| v["evidence_type"] == "official_group_sent")
            .count(),
        1
    );
    assert!(rows.iter().all(|v| v.get("result_credential").is_none()));
    assert!(
        sqlx::query("DELETE FROM channel_execution_observations WHERE task_id=$1")
            .bind(task.id)
            .execute(&pool)
            .await
            .is_err()
    );
    let targets = service
        .rows(&a, task.id, "targets", Page::default())
        .await
        .unwrap();
    assert!(
        targets
            .iter()
            .any(|v| v["evidence"]["evidence_type"] == "official_group_sent")
    );
    let mut outsider = a.clone();
    outsider.id = Uuid::new_v4();
    outsider.platform_admin = false;
    assert!(
        service
            .rows(&outsider, task.id, "observations", Page::default())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn global_official_usage_revalidates_identity_and_confirmation_is_idempotent() {
    use wekeeper_api::distribution::{
        domain::workflows::{Command as C, Confirmation, UsageInput},
        north::local::workflows::Service as W,
        south::adapter::postgres::workflows::Repository as R,
    };
    let (pool, actor, _, _) = setup().await;
    let member = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1")
        .bind(actor.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let groups: Vec<Uuid> =
        sqlx::query_scalar("SELECT directory_group_id FROM wecom_groups ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let service = W(R(pool.clone()));
    let input = UsageInput {
        member_id: member,
        account_id: actor.id,
        generation: 1,
        group_ids: groups.clone(),
    };
    let preview = service
        .command(&actor, C::UsagePreview(input.clone()))
        .await
        .unwrap();
    let confirm = Confirmation {
        preview_id: preview["id"].as_str().unwrap().parse().unwrap(),
        digest: preview["digest"].as_str().unwrap().into(),
    };
    let original: serde_json::Value =
        sqlx::query_scalar("SELECT data FROM wecom_asset_objects WHERE id=$1")
            .bind(groups[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE wecom_asset_objects SET data=jsonb_set(data,'{owner}','\"changed\"') WHERE id=$1",
    )
    .bind(groups[0])
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        service
            .command(&actor, C::UsageConfirm(confirm))
            .await
            .is_err()
    );
    sqlx::query("UPDATE wecom_asset_objects SET data=$2 WHERE id=$1")
        .bind(groups[0])
        .bind(original)
        .execute(&pool)
        .await
        .unwrap();
    let preview = service
        .command(&actor, C::UsagePreview(input))
        .await
        .unwrap();
    let confirm = Confirmation {
        preview_id: preview["id"].as_str().unwrap().parse().unwrap(),
        digest: preview["digest"].as_str().unwrap().into(),
    };
    let result = service
        .command(&actor, C::UsageConfirm(confirm.clone()))
        .await
        .unwrap();
    assert_eq!(result["state"], "approved");
    assert_eq!(
        service
            .command(&actor, C::UsageConfirm(confirm))
            .await
            .unwrap(),
        result
    );
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn sender_options_include_member_groups_without_unused_association_fields() {
    use wekeeper_api::distribution::south::port::workflows::Store as _;
    let (pool, a, _, fake) = setup().await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,data,checked_at,credential_generation,visibility,verification) VALUES($1,'groups',$2,'unbound',jsonb_build_object('owner','owner','name','未关联群'),now(),1,'visible','verified')")
        .bind(id).bind(fake.corp).execute(&pool).await.unwrap();
    let options =
        wekeeper_api::distribution::south::adapter::postgres::workflows::Repository(pool.clone())
            .options(&a)
            .await
            .unwrap();
    let groups = options["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|g| g.get("associated").is_none()));
    assert!(groups.iter().any(|g| g["id"] == id.to_string()));
    pool.close().await;
}

#[derive(Clone)]
struct SlowRead {
    inner: Fake,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Client for SlowRead {
    async fn generation(&self) -> Result<(String, i64)> {
        self.inner.generation().await
    }
    async fn verify(&self, w: &Work) -> Result<()> {
        self.inner.verify(w).await
    }
    async fn invoke(&self, w: &Work) -> Outcome {
        self.inner.invoke(w).await
    }
    async fn results(&self, w: &Work) -> Result<Results> {
        self.started.notify_one();
        self.release.notified().await;
        self.inner.results(w).await
    }
    async fn candidates(&self, start: i64, end: i64) -> Result<Vec<Value>> {
        self.inner.candidates(start, end).await
    }
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn blocked_official_read_does_not_delay_newly_confirmed_create() {
    let (pool, a, mut input, fake) = setup().await;
    let client = SlowRead {
        inner: fake.clone(),
        started: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    };
    let service = Service {
        store: Repository(pool.clone()),
        client: client.clone(),
        management_available: true,
    };
    sqlx::query("INSERT INTO distribution_resource_policies(resource_key,window_seconds,max_calls) VALUES($1,3600,10)")
        .bind(format!("wecom:{}:owner",fake.corp)).execute(&pool).await.unwrap();
    let mut read_job = None;
    for namespace in ["test.first", "test.second"] {
        input.text_content = Some(format!("通知 {namespace}"));
        let task = service.save(&a, None, input.clone()).await.unwrap();
        let mut body = action(Some(task.revision));
        body.preview_digest = Some(service.preview(&a, task.id).await.unwrap().digest);
        service
            .action(&a, task.id, "confirm", namespace, body)
            .await
            .unwrap();
        if namespace == "test.first" {
            service.run_lane_once(false).await.unwrap();
        } else {
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                service.run_lane_once(false),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
            break;
        }
        let reader = service.clone();
        let job = tokio::spawn(async move { reader.run_lane_once(true).await });
        tokio::time::timeout(std::time::Duration::from_secs(2), client.started.notified())
            .await
            .unwrap();
        // Keep this read blocked while a later task is confirmed and created.
        // The spawned task is released and checked after the loop below.
        read_job = Some(job);
    }
    client.release.notify_one();
    let job = read_job.unwrap();
    job.await.unwrap().unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn qualification_router_uses_one_path_and_resource_type_with_live_authorization() {
    use tower::ServiceExt;
    let (pool, actor, _, _) = setup().await;
    let app = wekeeper_api::bootstrap::build_application(pool.clone());
    let (token, session) = app
        .identity
        .0
        .session(actor.id, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app.clone());
    let path = format!(
        "/api/v1/platform/wecom-distribution/wecom-senders/{}/qualification",
        actor.id
    );
    let send = |path: String, token: Option<String>, csrf: String, enabled: bool, revision: i64| {
        let router = router.clone();
        async move {
            let mut request = axum::http::Request::builder()
                .method("POST")
                .uri(path)
                .header("origin", "http://localhost:5173")
                .header("content-type", "application/vnd.api+json")
                .header("x-csrf-token", csrf);
            if let Some(token) = token {
                request = request.header("cookie", format!("wekeeper_session={token}"));
            }
            router.oneshot(request.body(axum::body::Body::from(json!({"data":{"type":"wecom-sender-qualifications","attributes":{"enabled":enabled,"revision":revision}}}).to_string())).unwrap()).await.unwrap()
        }
    };
    assert_eq!(
        send(path.clone(), None, session.csrf.clone(), false, 0)
            .await
            .status(),
        401
    );
    assert_eq!(
        send(
            format!(
                "/api/v1/wecom-distribution/wecom-senders/{}/qualification",
                actor.id
            ),
            Some(token.clone()),
            session.csrf.clone(),
            false,
            0
        )
        .await
        .status(),
        404
    );
    let response = send(
        path.clone(),
        Some(token.clone()),
        session.csrf.clone(),
        false,
        0,
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["data"]["type"], "wecom-sender-qualifications");
    assert_eq!(
        send(
            path.clone(),
            Some(token.clone()),
            session.csrf.clone(),
            true,
            0
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        send(
            path.clone(),
            Some(token.clone()),
            session.csrf.clone(),
            true,
            1
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE action='wecom.sender.qualification'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    // A captured administrator actor must not bypass current account permissions.
    sqlx::query("UPDATE accounts SET platform_admin=false WHERE id=$1")
        .bind(actor.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            path.clone(),
            Some(token.clone()),
            session.csrf.clone(),
            false,
            2
        )
        .await
        .status(),
        403
    );
    sqlx::query("UPDATE accounts SET active=false WHERE id=$1")
        .bind(actor.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(path, Some(token), session.csrf, false, 2)
            .await
            .status(),
        401
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn directory_and_usage_share_identity_rules_and_preview_freezes_enable_effects() {
    use wekeeper_api::{
        distribution::{
            domain::workflows::{Command as C, Confirmation, UsageInput},
            north::local::workflows::Service as W,
            south::adapter::postgres::workflows::Repository as R,
        },
        integrations::south::{
            adapter::directory_postgres::Repository as D, port::DirectoryStore as _,
        },
    };
    let (pool, actor, _, _) = setup().await;
    let directory = D(pool.clone());
    let member: Uuid = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1")
        .bind(actor.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let groups: Vec<Uuid> =
        sqlx::query_scalar("SELECT directory_group_id FROM wecom_groups ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let workflow = W(R(pool.clone()));
    let input = UsageInput {
        account_id: actor.id,
        member_id: member,
        generation: 1,
        group_ids: groups.clone(),
    };
    // Unlink needs no usable directory snapshot. Active administrators are valid binding targets.
    directory
        .bind(actor.id, actor.id, None, 0, None)
        .await
        .unwrap();
    for invalid in [
        "status=2",
        "visibility='not_seen'",
        "verification='unavailable'",
        "checked_at=now()-interval '25 hours'",
    ] {
        sqlx::query(&format!("UPDATE wecom_members SET {invalid} WHERE id=$1"))
            .bind(member)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            directory
                .bind(actor.id, actor.id, Some(member), 1, Some(1))
                .await
                .is_err(),
            "{invalid}"
        );
        assert!(
            workflow
                .command(&actor, C::UsagePreview(input.clone()))
                .await
                .is_err(),
            "{invalid}"
        );
        sqlx::query("UPDATE wecom_members SET status=1,visibility='visible',verification='verified',checked_at=now()-interval '2 hours' WHERE id=$1").bind(member).execute(&pool).await.unwrap();
    }
    // Both entries accept the same two-hour snapshot; binding alone does not restore qualification.
    sqlx::query("INSERT INTO wecom_sender_qualifications(account_id,enabled,updated_by) VALUES($1,false,$1)").bind(actor.id).execute(&pool).await.unwrap();
    directory
        .bind(actor.id, actor.id, Some(member), 1, Some(1))
        .await
        .unwrap();
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT enabled FROM wecom_sender_qualifications WHERE account_id=$1"
        )
        .bind(actor.id)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    directory
        .bind(actor.id, actor.id, None, 2, None)
        .await
        .unwrap();
    sqlx::query("UPDATE wecom_groups SET active=false")
        .execute(&pool)
        .await
        .unwrap();
    let preview = workflow
        .command(&actor, C::UsagePreview(input.clone()))
        .await
        .unwrap();
    assert_eq!(preview["summary"]["effects"]["bind_identity"], true);
    assert_eq!(preview["summary"]["effects"]["enable_qualification"], true);
    assert_eq!(
        preview["summary"]["effects"]["restore_group_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let confirm = |p: &Value| Confirmation {
        preview_id: p["id"].as_str().unwrap().parse().unwrap(),
        digest: p["digest"].as_str().unwrap().into(),
    };
    sqlx::query("UPDATE wecom_sender_qualifications SET revision=revision+1 WHERE account_id=$1")
        .bind(actor.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        workflow
            .command(&actor, C::UsageConfirm(confirm(&preview)))
            .await
            .is_err()
    );
    let preview = workflow
        .command(&actor, C::UsagePreview(input.clone()))
        .await
        .unwrap();
    workflow
        .command(&actor, C::UsageConfirm(confirm(&preview)))
        .await
        .unwrap();
    let revision: i64 =
        sqlx::query_scalar("SELECT wecom_binding_revision FROM accounts WHERE id=$1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revision, 4);
    // A new no-op preview stays idempotent and does not silently increase binding revisions.
    let preview = workflow
        .command(&actor, C::UsagePreview(input))
        .await
        .unwrap();
    assert_eq!(preview["summary"]["effects"]["bind_identity"], false);
    assert_eq!(preview["summary"]["effects"]["enable_qualification"], false);
    workflow
        .command(&actor, C::UsageConfirm(confirm(&preview)))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT wecom_binding_revision FROM accounts WHERE id=$1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        revision
    );
    pool.close().await;
}
