//! Requires an explicitly configured isolated database.
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use wekeeper_api::message_orchestration::{
    domain::{AcceptMessage, Error, EventKind, OriginalMessage},
    north::local::MessageIntake,
    south::adapter::postgres::Repository,
};

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn intake_is_idempotent_immutable_and_scoped_to_business_identity() {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(
            &std::env::var("WEKEEPER_TEST_DATABASE_URL")
                .expect("explicit isolated test database required"),
        )
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let actor = Uuid::new_v4();
    let platform = Uuid::new_v4();
    let other_platform = Uuid::new_v4();
    let unit = Uuid::new_v4();
    let other_unit = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'admin',true)",
    )
    .bind(actor)
    .bind(actor.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1),($2)")
        .bind(platform)
        .bind(other_platform)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'unit',$3,$3),($4,$5,'other',$3,$3)")
        .bind(unit).bind(platform).bind(actor).bind(other_unit).bind(other_platform).execute(&pool).await.unwrap();
    let input = AcceptMessage {
        platform_id: platform,
        unit_id: unit,
        source_event_id: Uuid::new_v4(),
        evidence_ref: Uuid::new_v4(),
        original: OriginalMessage {
            event_kind: EventKind::Upsert,
            external_content_id: "post-1".into(),
            version: "1".into(),
            sequence: 1,
            occurred_at: chrono::Utc::now(),
            title: Some("标题".into()),
            content: Some("正文".into()),
            detail_url: "https://example.org/1".into(),
        },
    };
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let service = MessageIntake(Repository(pool.clone()));
        let request = input.clone();
        tasks.push(tokio::spawn(async move {
            service.accept(request).await.unwrap()
        }));
    }
    let mut ids = Vec::new();
    for task in tasks {
        ids.push(task.await.unwrap().id);
    }
    assert!(ids.iter().all(|id| *id == ids[0]));
    let service = MessageIntake(Repository(pool.clone()));
    let mut modified = input.clone();
    modified.original.content = Some("changed".into());
    assert_eq!(service.accept(modified).await, Err(Error::Conflict));
    let mut modified = input.clone();
    modified.source_event_id = Uuid::new_v4();
    assert_eq!(service.accept(modified).await, Err(Error::Conflict));
    let mut modified = input.clone();
    modified.evidence_ref = Uuid::new_v4();
    assert_eq!(service.accept(modified).await, Err(Error::Conflict));
    let mut invalid = input.clone();
    invalid.source_event_id = Uuid::new_v4();
    invalid.evidence_ref = Uuid::new_v4();
    invalid.unit_id = other_unit;
    assert_eq!(
        service.accept(invalid.clone()).await,
        Err(Error::InvalidIdentity)
    );
    invalid.platform_id = other_platform;
    let other = service.accept(invalid).await.unwrap();
    assert_ne!(other.id, ids[0]);
    let mut bad = input.clone();
    bad.evidence_ref = Uuid::nil();
    assert_eq!(service.accept(bad).await, Err(Error::InvalidInput));
    let mut withdrawal = input.clone();
    withdrawal.source_event_id = Uuid::new_v4();
    withdrawal.evidence_ref = Uuid::new_v4();
    withdrawal.original.event_kind = EventKind::Withdrawn;
    withdrawal.original.sequence = 2;
    withdrawal.original.content = None;
    let accepted_withdrawal = service.accept(withdrawal.clone()).await.unwrap();
    assert_eq!(
        accepted_withdrawal.facts.original.event_kind,
        EventKind::Withdrawn
    );
    withdrawal.original.content = Some("not allowed".into());
    assert_eq!(service.accept(withdrawal).await, Err(Error::InvalidInput));
    let update = sqlx::query("UPDATE orchestration_messages SET original='{}' WHERE id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await;
    assert!(update.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orchestration_messages WHERE platform_id=$1")
            .bind(platform)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
}
