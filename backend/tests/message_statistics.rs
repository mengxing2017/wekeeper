//! Exercises real PostgreSQL aggregation and HTTP serialization against an isolated database.
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    message_statistics::{
        domain::{DetailFilter, Filter},
        north::local::Statistics,
        south::adapter::postgres::Repository,
    },
};
fn actor(id: Uuid) -> Actor {
    Actor {
        id,
        username: id.to_string(),
        display_name: "统计测试".into(),
        platform_admin: true,
        csrf: String::new(),
        session_hash: String::new(),
        request_id: Uuid::now_v7().to_string(),
        browser: false,
        wecom_authenticated: false,
        wecom_binding_revision: 0,
    }
}
async fn seed(pool: &PgPool, a: &Actor) -> (Uuid, Uuid, Uuid) {
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'统计测试',true)").bind(a.id).bind(a.id.to_string()).execute(pool).await.unwrap();
    let source = Uuid::now_v7();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,encryption_version,status,valid_from,created_by) VALUES($1,$2,'统计来源','v1','active',now(),$3)").bind(source.to_string()).bind(source).bind(a.id).execute(pool).await.unwrap();
    let route=wekeeper_api::source_access::north::local::Service(pool.clone()).manage(a,"source-routes",None,"POST",Some(serde_json::from_value(json!({"key_id":source.to_string(),"business_unit_name":"统计单位","status":"active"})).unwrap()),1,"","",None).await.unwrap();
    let route = route["id"].as_str().unwrap();
    let unit: Uuid =
        sqlx::query_scalar("SELECT business_unit_id FROM source_routes WHERE route_id=$1")
            .bind(route)
            .fetch_one(pool)
            .await
            .unwrap();
    let receipt = Uuid::now_v7();
    sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,event_id,route_id,body_sha256,schema_version,business_unit_id) VALUES($1,$2,$3,$4,'test',2,$5)").bind(receipt).bind(source).bind(Uuid::now_v7()).bind(route).bind(unit).execute(pool).await.unwrap();
    (source, unit, receipt)
}
#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn aggregation_responsibilities_snapshot_export_and_authorization() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
        .expect("explicit isolated test database required");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let a = actor(Uuid::now_v7());
    let (source, unit, receipt) = seed(&pool, &a).await;
    let service = Statistics(Repository(pool.clone()));
    let filter = Filter {
        platform_id: Some(source),
        ..Default::default()
    };
    let first = service.create(&a, filter.clone()).await.unwrap();
    assert_eq!(first.summary.received, "1");
    assert_eq!(first.summary.incomplete, "1");
    assert_eq!(first.summary.targets.total, "0");
    assert_eq!(
        first
            .trend
            .iter()
            .map(|x| x.received.parse::<i64>().unwrap())
            .sum::<i64>(),
        1
    );
    assert_eq!(first.cards[0].count, "1");
    let details = service
        .details(
            &a,
            first.id,
            DetailFilter {
                object: "message".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(details.items[0].receipt_id, Some(receipt));
    sqlx::query("UPDATE source_receipts SET processing_stage='not_dispatched' WHERE receipt_id=$1")
        .bind(receipt)
        .execute(&pool)
        .await
        .unwrap();
    let blocked = service.create(&a, filter.clone()).await.unwrap();
    assert_eq!(blocked.summary.not_dispatched, "1");
    assert_eq!(blocked.summary.incomplete, "0");
    assert_eq!(blocked.summary.processing, "0");
    sqlx::query("UPDATE source_receipts SET processing_stage='received' WHERE receipt_id=$1")
        .bind(receipt)
        .execute(&pool)
        .await
        .unwrap();
    let message = Uuid::now_v7();
    let configuration = Uuid::now_v7();
    let decision = Uuid::now_v7();
    let target = Uuid::now_v7();
    sqlx::query("INSERT INTO orchestration_messages(id,platform_id,unit_id,source_event_id,evidence_ref,original) SELECT $1,source_id,business_unit_id,event_id,receipt_id,'{}'::jsonb FROM source_receipts WHERE receipt_id=$2").bind(message).bind(receipt).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO orchestration_dispatches(message_id,content,decision,plans,state,content_frozen_at) VALUES($1,'hello',jsonb_build_object('content','hello','reason','no_available_ad','selected_ad',null,'selection_count',null),'[]','delegated',clock_timestamp())").bind(message).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO channel_receiving_configurations(id,business_unit_id,channel,configuration,authorized_by) VALUES($1,$2,'mobile','{}',$3)").bind(configuration).bind(unit).bind(a.id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO receiving_dispatch_decisions(id,message_id,configuration_id,channel,configuration,plan) VALUES($1,$2,$3,'mobile','{}','{}')").bind(decision).bind(message).bind(configuration).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO receiving_dispatch_targets(id,decision_id,message_id,channel,target_id,target,plan,expected_count,not_before) VALUES($1,$2,$3,'mobile',$4,$5,'{}',3,now())").bind(target).bind(decision).bind(message).bind(Uuid::now_v7()).bind(json!({"name":"统计群","client_kind":"wechat"})).execute(&pool).await.unwrap();
    assert!(service.overview(&a, first.id).await.is_err());
    let second = service.create(&a, filter.clone()).await.unwrap();
    assert_eq!(second.summary.targets.total, "1");
    assert_eq!(second.summary.targets.pending, "1");
    assert_eq!(second.summary.responsibilities.total, "3");
    assert_eq!(second.summary.responsibilities.pending, "3");
    assert!(
        second
            .channels
            .iter()
            .any(|c| c.id == "mobile:wechat" && c.targets.total == "1")
    );
    let detail = service
        .details(
            &a,
            second.id,
            DetailFilter {
                object: "responsibility".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(detail.items.len(), 3);
    let exported = service.export(&a, second.id).await.unwrap();
    assert!(exported.content.contains("统计单位"));
    assert!(exported.content.contains("Asia/Shanghai"));
    let audit:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit_events WHERE object_id=$1 AND action='message_statistics.export')").bind(second.id).fetch_one(&pool).await.unwrap();
    assert!(audit);
    let device = Uuid::now_v7();
    let delivery = Uuid::now_v7();
    let intake = Uuid::now_v7();
    sqlx::query("INSERT INTO receiving_dispatch_deliveries(id,dispatch_target_id,device_id,position) VALUES($1,$2,$3,0)").bind(delivery).bind(target).bind(device).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO mobile_dispatch_intakes(id,source_kind,message_id,receipt_id,target_id,target_name,snapshot_id,plan,content,payload_hash,creation_state,reason_code,delivery_id) SELECT $1,'source_route',message_id,$2,target_id,'统计群',$3,'{}','hello','hash','rejected','no_resource',$4 FROM receiving_dispatch_targets WHERE id=$5").bind(intake).bind(receipt).bind(Uuid::now_v7()).bind(delivery).bind(target).execute(&pool).await.unwrap();
    sqlx::query("UPDATE receiving_dispatch_targets SET materialized_count=1 WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    let partial = service.create(&a, filter.clone()).await.unwrap();
    assert_eq!(partial.summary.targets.pending, "1");
    assert_eq!(partial.summary.responsibilities.failed, "1");
    assert_eq!(partial.summary.responsibilities.pending, "2");
    let bucket = partial
        .trend
        .iter()
        .find(|b| b.received == "1")
        .unwrap()
        .start;
    let bucket_details = service
        .details(
            &a,
            partial.id,
            DetailFilter {
                object: "message".into(),
                bucket: Some(bucket),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(bucket_details.items.len(), 1);
    // Stopping an unmaterialized responsibility settles only that responsibility, not the source receipt.
    sqlx::query("UPDATE receiving_dispatch_targets SET stopped=true WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    let stopped = service.create(&a, filter.clone()).await.unwrap();
    assert_eq!(stopped.summary.targets.failed, "1");
    assert_eq!(stopped.summary.responsibilities.failed, "3");
    assert_eq!(stopped.summary.received, "1");
    let channel_filtered = service
        .create(
            &a,
            Filter {
                channel: Some("wecom".into()),
                ..filter.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(channel_filtered.cards[0].count, "1");
    assert_eq!(channel_filtered.summary.targets.total, "0");
    let other = actor(Uuid::now_v7());
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'other',true)",
    )
    .bind(other.id)
    .bind(other.id.to_string())
    .execute(&pool)
    .await
    .unwrap();
    assert!(service.overview(&other, stopped.id).await.is_err());
    sqlx::query("UPDATE statistics_query_snapshots SET expires_at=$2 WHERE id=$1")
        .bind(stopped.id)
        .bind(Utc::now() - Duration::seconds(1))
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.export(&a, stopped.id).await.is_err());
    sqlx::query("UPDATE accounts SET active=false WHERE id=$1")
        .bind(a.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.create(&a, filter).await.is_err());
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn official_acceptance_is_not_sent_and_manual_tasks_do_not_create_receipts() {
    let pool = PgPoolOptions::new()
        .connect(
            &std::env::var("WEKEEPER_TEST_DATABASE_URL")
                .expect("explicit isolated test database required"),
        )
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let a = actor(Uuid::now_v7());
    seed(&pool, &a).await;
    let task = Uuid::now_v7();
    let batch = Uuid::now_v7();
    let target = Uuid::now_v7();
    let group = Uuid::now_v7();
    sqlx::query("INSERT INTO wecom_groups(id,corp_id,chat_id,name) VALUES($1,$2,$3,'同名群')")
        .bind(group)
        .bind(group.to_string())
        .bind(group.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO broadcast_tasks(id,creator_id,content,content_hash,state,corp_id,generation,confirmed_by,confirmed_at,draft_input) VALUES($1,$2,'hello','hash','confirmed','corp',1,$2,now(),'{}')").bind(task).bind(a.id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_batches(id,task_id,ordinal,sender,sender_account,binding_revision,state,corp_id) VALUES($1,$2,1,'official',$3,1,'created','corp')").bind(batch).bind(task).bind(a.id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO broadcast_targets(id,task_id,batch_id,group_id,chat_id,group_name,group_revision) VALUES($1,$2,$3,$4,'chat','冻结群名',1)").bind(target).bind(task).bind(batch).bind(group).execute(&pool).await.unwrap();
    let service = Statistics(Repository(pool.clone()));
    let f = Filter {
        source_kind: "manual".into(),
        channel: Some("wecom".into()),
        ..Default::default()
    };
    let first = service.create(&a, f.clone()).await.unwrap();
    assert_eq!(first.summary.received, "0");
    assert_eq!(first.summary.targets.pending, "1");
    assert_eq!(first.summary.targets.sent, "0");
    assert_eq!(first.units[0].id, "unassigned");
    assert_eq!(first.units[0].received, "0");
    let details = service
        .details(
            &a,
            first.id,
            DetailFilter {
                object: "target".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(details.items[0].task_id, Some(task));
    assert_eq!(details.items[0].target_name.as_deref(), Some("冻结群名"));
    sqlx::query("UPDATE broadcast_targets SET official_status=1 WHERE id=$1")
        .bind(target)
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.export(&a, first.id).await.is_err());
    let second = service.create(&a, f).await.unwrap();
    assert_eq!(second.summary.targets.sent, "1");
    assert_eq!(second.summary.responsibilities.sent, "1");
}
