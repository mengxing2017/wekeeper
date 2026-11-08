use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{bootstrap, database};

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn wecom_permissions_ignore_schools_and_do_not_grant_mobile_access() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated DB required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("wecom_isolation_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
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
    let app = bootstrap::build_application(pool.clone());
    let root = app
        .identity
        .bootstrap("root", "Root", "test-password-123", true)
        .await
        .unwrap();
    let uid = app
        .identity
        .bootstrap("operator", "Operator", "test-password-123", false)
        .await
        .unwrap();
    let (_, admin) = app
        .identity
        .0
        .session(root, "browser", false)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    let other = Uuid::new_v4();
    app.wecom_workspaces()
        .save(&admin, id, "Independent")
        .await
        .unwrap();
    app.wecom_workspaces()
        .save(&admin, other, "Other")
        .await
        .unwrap();
    app.wecom_workspaces()
        .grant(&admin, id, "operator", "operator")
        .await
        .unwrap();
    let (token, actor) = app.identity.0.session(uid, "browser", false).await.unwrap();
    assert!(actor.bound_school_id.is_none());
    assert!(app.distribution_groups().groups(&actor, id).await.is_ok());
    assert!(
        app.distribution_groups()
            .groups(&actor, other)
            .await
            .is_err()
    );
    assert!(
        app.mobile_workspaces()
            .list(&actor)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT to_regclass('schools') IS NULL AND to_regclass('school_memberships') IS NULL"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    let actor = app.identity.authenticate(&token, "browser").await.unwrap();
    assert!(app.distribution_groups().groups(&actor, id).await.is_ok());
    app.wecom_workspaces()
        .grant(&admin, id, "operator", "revoked")
        .await
        .unwrap();
    let actor = app.identity.authenticate(&token, "browser").await.unwrap();
    assert!(actor.grants.is_empty());
    assert!(app.distribution_groups().groups(&actor, id).await.is_err());
    assert!(
        app.wecom_workspaces()
            .list(&actor)
            .await
            .unwrap()
            .is_empty()
    );
    // Dedicated administrator reset cannot reset an ordinary distribution account.
    assert!(
        app.identity
            .reset(&admin, uid, "changed-password-123")
            .await
            .is_err()
    );
}
