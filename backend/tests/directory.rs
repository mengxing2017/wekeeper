use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wekeeper_api::{
    bootstrap,
    business::Page,
    database,
    integrations::{
        domain::directory::*,
        north::directory::Service,
        south::{
            adapter::directory_postgres::Repository,
            port::{DirectoryClient, DirectoryStore},
        },
    },
};
#[derive(Clone)]
struct Fake(Arc<Mutex<(Vec<Member>, bool)>>);
impl DirectoryClient for Fake {
    async fn departments(&self) -> Result<Vec<Department>, ReadError> {
        Ok(vec![
            Department {
                id: 2,
                parentid: 1,
                name: "运营部".into(),
                order: 0,
            },
            Department {
                id: 3,
                parentid: 1,
                name: "编辑部".into(),
                order: 0,
            },
        ])
    }
    async fn members(&self, _: i64) -> Result<Vec<Member>, ReadError> {
        let state = self.0.lock().unwrap();
        if state.1 {
            Err(ReadError::Transient)
        } else {
            Ok(state.0.clone())
        }
    }
    async fn member(&self, id: &str) -> Result<Member, ReadError> {
        let state = self.0.lock().unwrap();
        if state.1 {
            return Err(ReadError::Transient);
        }
        state
            .0
            .iter()
            .find(|m| m.userid.eq_ignore_ascii_case(id))
            .cloned()
            .ok_or(ReadError::Rejected(60111))
    }
}
#[tokio::test]
#[ignore = "requires isolated WEKEEPER_TEST_DATABASE_URL"]
async fn directory_sync_binding_and_identity_boundaries() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated test DB required");
    let admin_pool = sqlx::PgPool::connect(&url).await.unwrap();
    let schema = format!("directory_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .unwrap();
    let search = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |conn, _| {
            let sql = format!("SET search_path TO {search}");
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    database::migrate(&pool).await.unwrap();
    let app = bootstrap::build_application(pool.clone());
    let admin_id = app
        .identity
        .bootstrap("admin", "Admin", "test-password-123", true)
        .await
        .unwrap();
    let (admin_token, admin) = app
        .identity
        .0
        .session(admin_id, "browser", false)
        .await
        .unwrap();
    let user = app
        .identity
        .bootstrap("user", "本地昵称", "test-password-123", false)
        .await
        .unwrap();
    let other = app
        .identity
        .bootstrap("other", "另一个账号", "test-password-123", false)
        .await
        .unwrap();
    let (ordinary_token, ordinary) = app
        .identity
        .0
        .session(user, "browser", false)
        .await
        .unwrap();
    let fake = Fake(Arc::new(Mutex::new((
        vec![Member {
            userid: "Zhang.San".into(),
            name: "张三".into(),
            status: Some(1),
            department: vec![2, 3],
            main_department: Some(2),
        }],
        false,
    ))));
    let svc = Service {
        store: Repository(pool.clone()),
        client: fake.clone(),
        corp: "corp".into(),
        credential_scope: "scope1".into(),
        server_configured: true,
    };
    svc.ensure_config().await.unwrap();
    assert!(svc.store.configure("other-corp", "1").await.is_err());
    assert!(
        svc.list(&ordinary, &DirectoryQuery::default(), false)
            .await
            .is_err()
    );
    assert!(svc.enqueue(&ordinary).await.is_err());
    let first = svc.enqueue(&admin).await.unwrap();
    assert_eq!(first["id"], svc.enqueue(&admin).await.unwrap()["id"]);
    svc.run_once(0).await.unwrap();
    let rows = svc
        .list(&admin, &DirectoryQuery::default(), false)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "multi-department member must not be duplicated"
    );
    let parsed: DirectoryMember = serde_json::from_value(rows[0].clone()).unwrap();
    assert_eq!(parsed.departments.len(), 2);
    assert_eq!(parsed.name, "张三");
    // Exercise the real HTTP permission and wire boundaries, not only services.
    let router = wekeeper_api::http::app_router(app.clone());
    let get = |token: String| {
        let router = router.clone();
        async move {
            use tower::ServiceExt;
            router
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/api/v1/platform/wecom-directory/members")
                        .header("cookie", format!("wekeeper_session={token}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    };
    assert_eq!(
        get(ordinary_token.clone()).await.status(),
        axum::http::StatusCode::FORBIDDEN
    );
    let response = get(admin_token.clone()).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["data"][0]["type"], "wecom-directory-members");
    assert_eq!(body["meta"]["page"]["has_more"], false);
    {
        use tower::ServiceExt;
        let response=router.clone().oneshot(axum::http::Request::builder().method("PUT").uri(format!("/api/v1/accounts/{user}")).header("cookie",format!("wekeeper_session={admin_token}")).header("origin","http://localhost:5173").header("x-csrf-token",&admin.csrf).header("content-type","application/vnd.api+json").body(axum::body::Body::from(serde_json::json!({"data":{"type":"accounts","attributes":{"display_name":"本地昵称","active":true,"wecom_userid":"bypass"}}}).to_string())).unwrap()).await.unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "manual UserID writes must not bypass verification"
        );
    }

    let member = parsed.id;
    assert!(
        svc.bind(
            &ordinary,
            user,
            BindingInput {
                member_id: member,
                revision: 0
            }
        )
        .await
        .is_err()
    );
    let (left, right) = tokio::join!(
        svc.bind(
            &admin,
            user,
            BindingInput {
                member_id: member,
                revision: 0
            }
        ),
        svc.bind(
            &admin,
            other,
            BindingInput {
                member_id: member,
                revision: 0
            }
        )
    );
    assert_ne!(
        left.is_ok(),
        right.is_ok(),
        "unique binding must be transactional"
    );
    let bound = if left.is_ok() { user } else { other };
    let (token, actor) = app
        .identity
        .0
        .session(bound, "browser", true)
        .await
        .unwrap();
    assert_eq!(actor.wecom_binding_revision, 1);
    assert_eq!(
        app.integrations
            .repository
            .member("ZHANG.SAN")
            .await
            .unwrap(),
        bound
    );
    assert!(
        !app.integrations
            .repository
            .session_member_matches(bound, 0, "Zhang.San", 1)
            .await
            .unwrap()
    );
    assert!(
        app.integrations
            .repository
            .session_member_matches(bound, 1, "Zhang.San", 1)
            .await
            .unwrap()
    );
    let before = svc.status(&admin).await.unwrap()["snapshot_id"].clone();
    fake.0.lock().unwrap().1 = true;
    svc.enqueue(&admin).await.unwrap();
    svc.run_once(0).await.unwrap();
    let status = svc.status(&admin).await.unwrap();
    assert_eq!(status["snapshot_id"], before);
    assert_eq!(status["latest_run"]["state"], "failed");
    assert!(svc.verify_account(bound).await.is_err());
    assert!(
        app.identity.authenticate(&token, "browser").await.is_ok(),
        "transient failure is not proof of revocation"
    );
    fake.0.lock().unwrap().1 = false;
    fake.0.lock().unwrap().0[0].status = Some(2);
    assert!(svc.verify_account(bound).await.is_err());
    assert!(app.identity.authenticate(&token, "browser").await.is_err());
    assert!(
        app.identity
            .0
            .session(bound, "browser", true)
            .await
            .is_err()
    );
    assert!(
        app.identity
            .0
            .session(bound, "browser", false)
            .await
            .is_ok(),
        "local account remains independent"
    );
    // A direct observation after a scan began must survive its publication.
    svc.enqueue(&admin).await.unwrap();
    let job = svc.store.claim(1).await.unwrap().unwrap();
    let mut active = fake.0.lock().unwrap().0[0].clone();
    active.status = Some(1);
    svc.store
        .observe(&active.userid, Some(&active), chrono::Utc::now(), 1)
        .await
        .unwrap();
    let stale = fake.0.lock().unwrap().0.clone();
    svc.store
        .finish(&job, &fake.departments().await.unwrap(), &stale)
        .await
        .unwrap();
    let rows = svc
        .list(&admin, &DirectoryQuery::default(), false)
        .await
        .unwrap();
    assert_eq!(rows[0]["status"], 1);
    // An expired worker cannot publish after another worker takes over.
    svc.enqueue(&admin).await.unwrap();
    let old = svc.store.claim(1).await.unwrap().unwrap();
    sqlx::query(
        "UPDATE wecom_directory_sync_runs SET lease_until=now()-interval '1 second' WHERE id=$1",
    )
    .bind(old.id)
    .execute(&pool)
    .await
    .unwrap();
    let new = svc.store.claim(1).await.unwrap().unwrap();
    assert_ne!(old.lease, new.lease);
    assert!(svc.store.finish(&old, &[], &[]).await.is_err());
    svc.store.fail(&old, "stale").await.unwrap();
    assert_eq!(svc.store.job(new.id).await.unwrap()["state"], "running");
    svc.store.fail(&new, "test_done").await.unwrap();
    // Rotation validates new credentials first and atomically fences every old writer.
    fake.0.lock().unwrap().0[0].status = Some(1);
    svc.verify_account(bound).await.unwrap();
    let (old_session, _) = app
        .identity
        .0
        .session(bound, "browser", true)
        .await
        .unwrap();
    let school = Uuid::now_v7();
    sqlx::query("INSERT INTO schools(id,name) VALUES($1,'Rotation school')")
        .bind(school)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO school_memberships(account_id,school_id,role) VALUES($1,$2,'operator')",
    )
    .bind(bound)
    .bind(school)
    .execute(&pool)
    .await
    .unwrap();
    svc.enqueue(&admin).await.unwrap();
    let old_job = svc.store.claim(1).await.unwrap().unwrap();
    let mut rotated = svc.clone();
    rotated.credential_scope = "scope2".into();
    assert!(rotated.ensure_config().await.is_err());
    assert_eq!(
        rotated.status(&admin).await.unwrap()["credential_rotation_required"],
        true
    );
    assert!(
        rotated
            .rotate(
                &ordinary,
                CredentialRotation {
                    expected_generation: 1
                }
            )
            .await
            .is_err()
    );
    fake.0.lock().unwrap().1 = true;
    assert!(
        rotated
            .rotate(
                &admin,
                CredentialRotation {
                    expected_generation: 1
                }
            )
            .await
            .is_err()
    );
    assert_eq!(svc.ensure_config().await.unwrap(), 1);
    fake.0.lock().unwrap().1 = false;
    let snapshot_before_rotation = svc.status(&admin).await.unwrap()["snapshot_id"].clone();
    rotated
        .rotate(
            &admin,
            CredentialRotation {
                expected_generation: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(rotated.ensure_config().await.unwrap(), 2);
    assert!(svc.ensure_config().await.is_err());
    assert!(
        rotated
            .rotate(
                &admin,
                CredentialRotation {
                    expected_generation: 1
                }
            )
            .await
            .is_err()
    );
    assert!(svc.store.configure("other-corp", "scope2").await.is_err());
    assert!(svc.store.renew(&old_job).await.is_err());
    assert!(svc.store.finish(&old_job, &[], &[]).await.is_err());
    assert!(svc.store.claim(1).await.is_err());
    assert!(svc.store.enqueue(Some(admin.id), 0, 1).await.is_err());
    assert!(
        svc.store
            .observe(&active.userid, Some(&active), chrono::Utc::now(), 1)
            .await
            .is_err()
    );
    assert!(
        svc.store
            .bind(admin.id, bound, Some(member), 1, Some(1))
            .await
            .is_err()
    );
    assert!(
        app.identity
            .authenticate(&old_session, "browser")
            .await
            .is_err()
    );
    assert!(
        app.identity
            .0
            .session(bound, "browser", true)
            .await
            .is_err()
    );
    assert_eq!(svc.store.account_userid(bound).await.unwrap(), "Zhang.San");
    assert_eq!(
        rotated.status(&admin).await.unwrap()["snapshot_id"],
        snapshot_before_rotation
    );
    rotated.run_once(0).await.unwrap();
    assert!(app.identity.0.session(bound, "browser", true).await.is_ok());
    assert!(
        !app.integrations
            .repository
            .session_member_matches(bound, 1, "Zhang.San", 1)
            .await
            .unwrap()
    );
    assert!(
        app.integrations
            .repository
            .session_member_matches(bound, 1, "Zhang.San", 2)
            .await
            .unwrap()
    );
    let svc = rotated;
    // Unknown status is stored, but never grants enterprise identity.
    fake.0.lock().unwrap().0[0].status = Some(99);
    assert!(svc.verify_account(bound).await.is_err());
    fake.0.lock().unwrap().0.clear();
    svc.enqueue(&admin).await.unwrap();
    svc.run_once(0).await.unwrap();
    let rows = svc
        .list(&admin, &DirectoryQuery::default(), false)
        .await
        .unwrap();
    assert_eq!(rows[0]["visibility"], "not_seen");
    assert!(svc.unbind(&admin, bound, 0).await.is_err());
    fake.0.lock().unwrap().1 = true;
    svc.unbind(&admin, bound, 1).await.unwrap();
    assert!(
        svc.store.account_userid(bound).await.is_err(),
        "unbind works without upstream"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM school_memberships")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT display_name FROM accounts WHERE id=$1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "本地昵称"
    );
    assert!(
        DirectoryQuery {
            page: Some(0),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert_eq!(Page::default().bounds().unwrap(), (30, 0));
    // Server-side filtering and pagination continue past the first page.
    fake.0.lock().unwrap().1 = false;
    fake.0.lock().unwrap().0 = (0..35)
        .map(|n| Member {
            userid: format!("user{n:02}"),
            name: format!("成员{n:02}"),
            status: Some(1),
            department: vec![2],
            main_department: Some(2),
        })
        .collect();
    svc.enqueue(&admin).await.unwrap();
    svc.run_once(0).await.unwrap();
    let q = DirectoryQuery {
        page: Some(2),
        search: Some("成员".into()),
        department_id: Some(2),
        status: Some(1),
        binding: Some("unbound".into()),
        ..Default::default()
    };
    assert_eq!(svc.list(&admin, &q, false).await.unwrap().len(), 5);
    let q = DirectoryQuery {
        search: Some("成员%".into()),
        ..Default::default()
    };
    assert!(
        svc.list(&admin, &q, false).await.unwrap().is_empty(),
        "search wildcard characters are literal"
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin_pool)
        .await
        .unwrap();
    admin_pool.close().await;
}
