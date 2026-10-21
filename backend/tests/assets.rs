use serde_json::json;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wekeeper_api::{
    bootstrap,
    business::Actor,
    database,
    integrations::{
        domain::{assets::*, directory::ReadError},
        north::assets::Service,
        south::{
            adapter::{assets_postgres::Repository, directory_postgres},
            port::{DirectoryStore, assets::AssetClient},
        },
    },
};
#[derive(Default)]
struct Scenario {
    invisible_owner: bool,
    page_error: bool,
    error: Option<i64>,
    loop_cursor: bool,
    empty: bool,
    owners: Vec<Vec<String>>,
}
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Scenario>>);
fn group(id: &str) -> Group {
    Group {
        chat_id: id.into(),
        name: Some("客户群".into()),
        owner: "Alice".into(),
        create_time: Some(1),
        member_version: Some("opaque-V1".into()),
        member_list: vec![
            GroupMember {
                userid: "Alice".into(),
                r#type: 1,
                join_time: Some(1),
                join_scene: None,
            },
            GroupMember {
                userid: "EXTERNAL".into(),
                r#type: 2,
                join_time: None,
                join_scene: None,
            },
            GroupMember {
                userid: "external".into(),
                r#type: 2,
                join_time: None,
                join_scene: None,
            },
        ],
    }
}
impl AssetClient for Fake {
    async fn groups(&self, owners: &[String], cursor: &str) -> Result<GroupPage, ReadError> {
        let mut s = self.0.lock().unwrap();
        s.owners.push(owners.to_vec());
        if let Some(code) = s.error {
            return Err(ReadError::Rejected(code));
        }
        if s.page_error && !cursor.is_empty() {
            return Err(ReadError::Rejected(60020));
        }
        if s.invisible_owner && owners.iter().any(|o| o == "Departed") {
            return Err(ReadError::Rejected(60111));
        }
        Ok(GroupPage {
            group_chat_list: if s.empty {
                vec![]
            } else {
                vec![GroupRef {
                    chat_id: "CHAT".into(),
                    status: 0,
                }]
            },
            next_cursor: if cursor.is_empty() || s.loop_cursor {
                "next"
            } else {
                ""
            }
            .into(),
        })
    }
    async fn group(&self, id: &str) -> Result<Group, ReadError> {
        Ok(group(id))
    }
}
async fn actor(app: &bootstrap::Application, name: &str, platform: bool) -> Actor {
    let id = app
        .identity
        .bootstrap(name, name, "test-password-123", platform)
        .await
        .unwrap();
    app.identity
        .0
        .session(id, "browser", false)
        .await
        .unwrap()
        .1
}
async fn run(svc: &Service<Fake>, dataset: &str) -> AssetJob {
    svc.store.enqueue(None, 1, dataset, 0).await.unwrap();
    let j = svc.store.claim(1).await.unwrap().unwrap();
    assert_eq!(j.dataset, dataset);
    svc.run_job(&j).await.unwrap();
    j
}
async fn count(pool: &sqlx::PgPool, kind: &str, verified: bool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM wecom_asset_objects WHERE kind=$1 AND (NOT $2 OR verification='verified')").bind(kind).bind(verified).fetch_one(pool).await.unwrap()
}
#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL"]
async fn asset_snapshots_leases_scope_and_school_boundaries() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated test database required");
    let admin_pool = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("assets_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .unwrap();
    let search = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |c, _| {
            let sql = format!("SET search_path TO {search}");
            Box::pin(async move {
                sqlx::query(&sql).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let app = bootstrap::build_application(pool.clone());
    let admin = actor(&app, "admin", true).await;
    let ordinary = actor(&app, "ordinary", false).await;
    let dir = directory_postgres::Repository(pool.clone());
    dir.configure("corp", "scope").await.unwrap();
    // Complete current-generation internal directory is a prerequisite for exhaustive owners.
    dir.enqueue(Some(admin.id), 0, 1).await.unwrap();
    let dj = dir.claim(1).await.unwrap().unwrap();
    let members = (0..101)
        .map(|i| wekeeper_api::integrations::domain::directory::Member {
            userid: if i == 0 {
                "Alice".into()
            } else if i == 1 {
                "Bob".into()
            } else {
                format!("Member{i}")
            },
            name: format!("成员{i}"),
            status: Some(1),
            department: vec![],
            main_department: None,
        })
        .collect::<Vec<_>>();
    dir.finish(&dj, &[], &members).await.unwrap();
    let fake = Fake::default();
    let svc = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        integration: app.integrations.clone(),
    };
    assert!(
        svc.list(&ordinary, "groups", &AssetQuery::default(), None, None)
            .await
            .is_err()
    );
    let groups = run(&svc, "groups").await;
    assert_eq!(count(&pool, "groups", true).await, 1);
    assert_eq!(count(&pool, "members", true).await, 3);
    assert!(fake.0.lock().unwrap().owners.iter().all(|v| v.len() <= 100));
    assert!(fake.0.lock().unwrap().owners.iter().any(|v| v.len() == 100));
    let rows = svc
        .list(&admin, "groups", &AssetQuery::default(), None, None)
        .await
        .unwrap();
    // Compare a real PostgreSQL response through the Rust JSON:API serializer with the TS fixture.
    let mut wire = rows[0].clone();
    wire["id"] = json!(Uuid::from_u128(1));
    wire["member_id"] = json!(Uuid::from_u128(2));
    wire["checked_at"] = json!("2026-10-21T00:00:00+00:00");
    use axum::response::IntoResponse;
    let response = wekeeper_api::http::jsonapi::collection(
        "wecom-asset-groups",
        vec![wire],
        &Default::default(),
    )
    .into_response();
    let actual: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 16384)
            .await
            .unwrap(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/wecom-asset-group.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
    let gid: Uuid = rows[0]["id"].as_str().unwrap().parse().unwrap();
    let gm = svc
        .list(&admin, "members", &AssetQuery::default(), Some(gid), None)
        .await
        .unwrap();
    assert_eq!(gm.iter().filter(|v| v["member_type"] == 2).count(), 2);
    assert!(gm.iter().all(|v| v.get("customer_id").is_none()));
    assert!(gm.iter().all(|v| v.get("type").is_none()));
    let mut member = gm.iter().find(|v| v["userid"] == "Alice").unwrap().clone();
    member["id"] = json!(Uuid::from_u128(1));
    member["member_id"] = json!(Uuid::from_u128(2));
    member["checked_at"] = json!("2026-10-21T00:00:00+00:00");
    let response = wekeeper_api::http::jsonapi::collection(
        "wecom-asset-members",
        vec![member],
        &Default::default(),
    )
    .into_response();
    let actual: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 16384)
            .await
            .unwrap(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/wecom-asset-member.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);

    let stats = svc
        .list(
            &admin,
            "member-statistics",
            &AssetQuery::default(),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(stats[0].get("customer_count").is_none());
    assert_eq!(stats[0]["owned_group_count"], 1);
    // Mid-pagination failures and cursor loops never publish or mark old records absent.
    fake.0.lock().unwrap().loop_cursor = true;
    let failure = run(&svc, "groups").await;
    let status = svc
        .store
        .list(
            "sync-runs",
            &AssetQuery::default(),
            None,
            Some(failure.fence.id),
        )
        .await
        .unwrap();
    assert_eq!(status[0]["error_code"], "cursor_loop");
    assert_eq!(count(&pool, "groups", true).await, 1);
    fake.0.lock().unwrap().loop_cursor = false;
    fake.0.lock().unwrap().page_error = true;
    let middle = run(&svc, "groups").await;
    assert_eq!(
        svc.store
            .list(
                "sync-runs",
                &AssetQuery::default(),
                None,
                Some(middle.fence.id)
            )
            .await
            .unwrap()[0]["error_code"],
        "wecom_60020"
    );
    assert_eq!(count(&pool, "members", true).await, 3);
    fake.0.lock().unwrap().page_error = false;
    fake.0.lock().unwrap().error = Some(60020);
    let failed = run(&svc, "groups").await;
    assert_eq!(
        svc.store
            .list(
                "sync-runs",
                &AssetQuery::default(),
                None,
                Some(failed.fence.id)
            )
            .await
            .unwrap()[0]["error_code"],
        "wecom_60020"
    );
    fake.0.lock().unwrap().error = None;
    let school = app.schools.create(&admin, "School A").await.unwrap();
    let school: Uuid = school["id"].as_str().unwrap().parse().unwrap();
    let other = app.schools.create(&admin, "School B").await.unwrap();
    let other: Uuid = other["id"].as_str().unwrap().parse().unwrap();
    let candidates = svc
        .store
        .candidates(&admin, school, &AssetQuery::default(), 1)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    let at: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(candidates[0]["checked_at"].clone()).unwrap();
    let binding = GroupBinding {
        directory_group_id: gid,
        expected_generation: 1,
        expected_checked_at: at,
        name: Some("学校显示名".into()),
    };
    assert!(
        svc.store
            .bind_group(&ordinary, school, &binding, &group("CHAT"))
            .await
            .is_err()
    );
    // One group, one school: concurrent administrators cannot overwrite another mapping.
    let official = group("CHAT");
    let (left, right) = tokio::join!(
        svc.store.bind_group(&admin, school, &binding, &official),
        svc.store.bind_group(&admin, other, &binding, &official)
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let bound_school = if left.is_ok() { school } else { other };
    let denied_school = if left.is_ok() { other } else { school };
    assert!(
        svc.store
            .candidates(&admin, denied_school, &AssetQuery::default(), 1)
            .await
            .unwrap()
            .is_empty()
    );
    // Point verification cannot be overwritten by an older complete scan, including its member relations.
    svc.store.enqueue(None, 1, "groups", 0).await.unwrap();
    let old = svc.store.claim(1).await.unwrap().unwrap();
    let mut newer = group("CHAT");
    newer.name = Some("较新官方群名".into());
    newer.member_list.truncate(1);
    svc.store
        .observe(1, &newer, chrono::Utc::now())
        .await
        .unwrap();
    svc.run_job(&old).await.unwrap();
    let current = svc
        .store
        .list("groups", &AssetQuery::default(), None, Some(gid))
        .await
        .unwrap();
    assert_eq!(current[0]["name"], "较新官方群名");
    assert_eq!(count(&pool, "members", true).await, 1);
    let name: String = sqlx::query_scalar("SELECT name FROM wecom_groups WHERE school_id=$1")
        .bind(bound_school)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "学校显示名");
    assert!(
        svc.store
            .bind_group(&admin, school, &binding, &newer)
            .await
            .is_err()
    );
    // Expired lease: clear staging, new scan time, old staging/renew/publication rejected.
    svc.store.enqueue(None, 1, "groups", 0).await.unwrap();
    let old = svc.store.claim(1).await.unwrap().unwrap();
    svc.store
        .stage(&old, "group-ids", "old", json!({}))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE wecom_asset_sync_runs SET lease_until=now()-interval '1 second' WHERE id=$1",
    )
    .bind(old.fence.id)
    .execute(&pool)
    .await
    .unwrap();
    let new = svc.store.claim(1).await.unwrap().unwrap();
    assert_ne!(new.fence.lease, old.fence.lease);
    assert!(
        svc.store
            .items(&new, "group-ids", "")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        svc.store
            .stage(&old, "group-ids", "late", json!({}))
            .await
            .is_err()
    );
    assert!(svc.store.renew(&old).await.is_err());
    assert!(svc.store.finish(&old).await.is_err());
    svc.run_job(&new).await.unwrap();
    // Historic invisible owners stop publication until an explicit, audited scope decision.
    fake.0.lock().unwrap().empty = false;
    let mut historic = group("HISTORIC");
    historic.owner = "Departed".into();
    svc.store
        .observe(1, &historic, chrono::Utc::now())
        .await
        .unwrap();
    fake.0.lock().unwrap().invisible_owner = true;
    let failed_owner_run = run(&svc, "groups").await;
    let failed = svc
        .store
        .list(
            "sync-runs",
            &AssetQuery::default(),
            None,
            Some(failed_owner_run.fence.id),
        )
        .await
        .unwrap();
    assert_eq!(failed[0]["failed_owner"], "Departed");
    assert_eq!(failed[0]["error_code"], "wecom_60111");
    svc.store
        .confirm_scope(
            &admin,
            &OwnerScopeDecision {
                run_id: failed_owner_run.fence.id,
                owner_userid: "Departed".into(),
                expected_generation: 1,
                reason: "已核对该群主离职且不可见".into(),
            },
        )
        .await
        .unwrap();
    let recovered = run(&svc, "groups").await;
    assert_eq!(
        svc.store
            .list(
                "sync-runs",
                &AssetQuery::default(),
                None,
                Some(recovered.fence.id)
            )
            .await
            .unwrap()[0]["state"],
        "succeeded"
    );
    let history = svc
        .store
        .list(
            "groups",
            &AssetQuery {
                search: Some("HISTORIC".into()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(history[0]["verification"], "unverified");
    // Freshness is enforced on the server, not merely as a UI warning.
    sqlx::query("UPDATE wecom_asset_objects SET checked_at=now()-interval '25 hours' WHERE kind IN ('groups','members')").execute(&pool).await.unwrap();
    assert!(
        svc.store
            .candidates(&admin, bound_school, &AssetQuery::default(), 1)
            .await
            .unwrap()
            .is_empty()
    );
    let current = svc
        .store
        .list("member-statistics", &AssetQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(current[0]["owned_group_count"], 0);
    // Rotation cancels the group job in the same transaction and retains school mapping.
    svc.store
        .enqueue(Some(admin.id), 1, "groups", 0)
        .await
        .unwrap();
    let old = svc.store.claim(1).await.unwrap().unwrap();
    dir.rotate(admin.id, "corp", "new-scope", 1).await.unwrap();
    assert!(
        svc.store
            .stage(&old, "group-ids", "late", json!({}))
            .await
            .is_err()
    );
    assert!(svc.store.finish(&old).await.is_err());
    assert!(
        svc.store
            .candidates(&admin, school, &AssetQuery::default(), 2)
            .await
            .unwrap()
            .is_empty()
    );
    let mappings: i64 = sqlx::query_scalar("SELECT count(*) FROM wecom_groups")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mappings, 1);
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wecom_asset_sync_runs WHERE state='queued' AND generation=2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    let _ = groups;
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin_pool)
        .await
        .unwrap();
    admin_pool.close().await;
}
#[test]
fn group_identity_validation_and_strict_queries() {
    assert!(
        SyncInput {
            dataset: "groups".into()
        }
        .validate()
        .is_ok()
    );
    for dataset in ["customers", "all"] {
        assert!(
            SyncInput {
                dataset: dataset.into()
            }
            .validate()
            .is_err()
        );
        assert!(
            AssetQuery {
                dataset: Some(dataset.into()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    let mut g = group("CHAT");
    assert!(g.validate("CHAT").is_ok());
    assert!(g.validate("chat").is_err());
    g.member_list[0].r#type = 3;
    assert!(g.validate("CHAT").is_err());
    assert!(serde_json::from_value::<Group>(json!({"chat_id":"a","owner":"Alice"})).is_err());
    assert!(serde_json::from_value::<AssetQuery>(json!({"corp_id":"another"})).is_err());
    assert!(serde_json::from_value::<SyncInput>(json!({"dataset":"all","secret":"x"})).is_err());
}
