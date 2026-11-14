use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::{
    business::Actor,
    distribution::{
        domain::receiving::{Channel, ConfigurationInput},
        north::local::receiving::Service,
        south::adapter::postgres::receiving::Repository,
    },
    message_orchestration::{
        domain::{AcceptMessage, EventKind, OriginalMessage},
        north::local::MessageIntake,
    },
};
pub struct Fixture {
    pub pool: PgPool,
    base: PgPool,
    schema: String,
    pub actor: Actor,
    pub platform: Uuid,
    pub unit: Uuid,
    pub route: String,
    pub scope: Uuid,
    pub target: Uuid,
    pub device: Uuid,
}
impl Fixture {
    pub async fn new() -> Self {
        let url = std::env::var("WEKEEPER_TEST_DATABASE_URL")
            .expect("explicit isolated test database required");
        let base = PgPool::connect(&url).await.unwrap();
        let schema = format!("unit_dispatch_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&base)
            .await
            .unwrap();
        let search = format!("SET search_path TO {schema},public");
        let pool = PgPoolOptions::new()
            .max_connections(12)
            .after_connect(move |c, _| {
                let sql = search.clone();
                Box::pin(async move {
                    sqlx::query(&sql).execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        wekeeper_api::database::migrate(&pool).await.unwrap();
        let actor_id = Uuid::new_v4();
        let platform = Uuid::new_v4();
        let unit = Uuid::new_v4();
        let route = Uuid::new_v4().to_string();
        let scope = Uuid::new_v4();
        let target = Uuid::new_v4();
        let device = Uuid::new_v4();
        sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,'dispatch-admin','Admin',true)").bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
            .bind(platform)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$2,'单位',$3,$3)").bind(unit).bind(platform).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO source_routes(route_id,source_id,business_unit_id,route_name,status) VALUES($1,$2,$3,'消息','active')").bind(&route).bind(platform).bind(unit).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_workspaces(id,name,active) VALUES($1,'历史分区',false)")
            .bind(scope)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO execution_devices(id,name,enabled,credential_generation) VALUES($1,'Offline',true,1)").bind(device).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES($1,$2,'群名',$3,$3)").bind(target).bind(scope).bind(actor_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)").bind(Uuid::new_v4()).bind(scope).bind(device).bind(target).bind(actor_id).execute(&pool).await.unwrap();
        let actor:Actor=serde_json::from_value(json!({"id":actor_id,"username":"dispatch-admin","display_name":"Admin","platform_admin":true,"grants":[],"wecom_authenticated":false})).unwrap();
        Self {
            pool,
            base,
            schema,
            actor,
            platform,
            unit,
            route,
            scope,
            target,
            device,
        }
    }
    pub async fn configure(&self, targets: Vec<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        Service(Repository(self.pool.clone()))
            .save(
                &self.actor,
                Channel::Mobile,
                self.unit,
                id,
                ConfigurationInput {
                    name: "接收配置".into(),
                    enabled: true,
                    target_ids: targets,
                    device_ids: vec![self.device],
                    sender_ids: vec![],
                },
            )
            .await
            .unwrap();
        id
    }
    pub async fn message(&self, sequence: i64) -> (Uuid, Uuid) {
        let receipt = Uuid::new_v4();
        let event = Uuid::new_v4();
        let original = OriginalMessage {
            event_kind: EventKind::Upsert,
            external_content_id: "same-content".into(),
            version: sequence.to_string(),
            sequence,
            occurred_at: chrono::Utc::now(),
            title: Some("标题".into()),
            content: Some("原消息".into()),
            detail_url: "https://example.org/message".into(),
        };
        let body=serde_json::to_vec(&json!({"data":{"type":"source-events","id":event,"attributes":{"event_id":event,"schema_version":2,"route_id":self.route,"event_type":"content.upsert","external_content_id":original.external_content_id,"version":original.version,"sequence":sequence,"occurred_at":original.occurred_at,"title":original.title,"content":original.content,"detail_url":original.detail_url}}})).unwrap();
        sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,business_unit_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,$5,$6,2)").bind(receipt).bind(self.platform).bind(self.unit).bind(event).bind(&self.route).bind(wekeeper_api::source_access::domain::digest(&body)).execute(&self.pool).await.unwrap();
        sqlx::query("INSERT INTO source_inbox(receipt_id,request_body) VALUES($1,$2)")
            .bind(receipt)
            .bind(body)
            .execute(&self.pool)
            .await
            .unwrap();
        let message = MessageIntake(
            wekeeper_api::message_orchestration::south::adapter::postgres::Repository(
                self.pool.clone(),
            ),
        )
        .accept(AcceptMessage {
            platform_id: self.platform,
            unit_id: self.unit,
            source_event_id: event,
            evidence_ref: receipt,
            original,
        })
        .await
        .unwrap();
        (message.id, receipt)
    }
    pub async fn run(&self) {
        wekeeper_api::message_orchestration::south::adapter::postgres::Repository(
            self.pool.clone(),
        )
        .run_once()
        .await
        .unwrap();
    }
    pub async fn ad(&self, body: &str, scope: &str) -> Uuid {
        let id = Uuid::new_v4();
        let version = Uuid::new_v4();
        sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
            .bind(id)
            .bind(self.actor.id)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,created_by) VALUES($1,$2,1,'广告',$3,$4)").bind(version).bind(id).bind(body).bind(self.actor.id).execute(&self.pool).await.unwrap();
        sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
            .bind(id)
            .bind(version)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ad_placements(id,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,created_by,updated_by) VALUES($1,$2,$3,$4,$5,true,now()-interval '1 day',$6,$6)").bind(Uuid::new_v4()).bind(id).bind(scope).bind(if scope=="global" {None}else{Some(self.platform)}).bind(if scope=="unit"{Some(self.unit)}else{None}).bind(self.actor.id).execute(&self.pool).await.unwrap();
        id
    }
    pub async fn close(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.base)
            .await
            .unwrap();
        self.base.close().await;
    }
}
