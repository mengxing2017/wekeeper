use crate::{
    business::{Actor, Error, Result},
    message_statistics::{domain::*, south::port::StatisticsStore},
};
use chrono::{DateTime, Duration, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub struct Repository(pub PgPool);
// Bound analytics concurrency independently from delivery processing.
static QUERIES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
type Tx<'a> = Transaction<'a, Postgres>;
fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Database(e.to_string()))
}
async fn begin(pool: &PgPool, actor: &Actor) -> Result<Tx<'static>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    let authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1 AND active AND platform_admin)",
    )
    .bind(actor.id)
    .fetch_one(&mut *tx)
    .await?;
    if !authorized {
        return Err(Error::Forbidden);
    }
    Ok(tx)
}
async fn prepare(tx: &mut Tx<'_>, f: &Filter, now: DateTime<Utc>) -> Result<String> {
    let filter = json!(f);
    sqlx::query(r#"CREATE TEMP TABLE stats_m ON COMMIT DROP AS SELECT m.* FROM statistics_messages m
 WHERE ($1::jsonb->>'platform_id' IS NULL OR m.platform_id=($1::jsonb->>'platform_id')::uuid)
 AND ($1::jsonb->>'business_unit_id' IS NULL OR m.business_unit_id=($1::jsonb->>'business_unit_id')::uuid)
 AND ($1::jsonb->>'route_id' IS NULL OR m.route_id=$1::jsonb->>'route_id') AND $1::jsonb->>'source_kind'<>'manual' AND m.cohort_at<$2"#).bind(&filter).bind(now).execute(&mut **tx).await?;
    sqlx::query(r#"CREATE TEMP TABLE stats_t ON COMMIT DROP AS SELECT t.* FROM statistics_targets t
 WHERE ($1::jsonb->>'platform_id' IS NULL OR t.platform_id=($1::jsonb->>'platform_id')::uuid)
 AND ($1::jsonb->>'business_unit_id' IS NULL OR t.business_unit_id=($1::jsonb->>'business_unit_id')::uuid)
 AND ($1::jsonb->>'route_id' IS NULL OR t.route_id=$1::jsonb->>'route_id')
 AND ($1::jsonb->>'source_kind'='all' OR t.source_kind=$1::jsonb->>'source_kind')
 AND ($1::jsonb->>'channel' IS NULL OR t.channel=$1::jsonb->>'channel')
 AND ($1::jsonb->>'client_kind' IS NULL OR t.client_kind=$1::jsonb->>'client_kind') AND t.cohort_at<$2"#).bind(&filter).bind(now).execute(&mut **tx).await?;
    sqlx::query("CREATE INDEX ON stats_t(receipt_id)")
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"CREATE TEMP TABLE stats_r ON COMMIT DROP AS
 SELECT t.id||':'||p.n id,t.receipt_id,coalesce(d.task_id,t.task_id) task_id,t.target_name,t.unit_name,t.channel,t.client_kind,t.cohort_at,
 CASE WHEN t.dispatch_target_id IS NULL THEN t.outcome ELSE coalesce(d.outcome,CASE WHEN rt.stopped THEN 'failed' ELSE 'pending' END) END outcome,
 coalesce(d.reason_code,t.reason_code) reason_code,
 coalesce(d.device_id,CASE WHEN rt.plan->>'device_dispatch_mode'='all' THEN (rt.plan->'device_ids'->>p.n)::uuid END) device_id,
 coalesce(d.historical_risk,t.historical_risk) historical_risk,t.business_unit_id
 FROM stats_t t CROSS JOIN LATERAL generate_series(0,t.expected::integer-1) p(n)
 LEFT JOIN receiving_dispatch_targets rt ON rt.id=t.dispatch_target_id
 LEFT JOIN receiving_delivery_progress d ON d.dispatch_target_id=t.dispatch_target_id AND d.position=p.n"#).execute(&mut **tx).await?;
    // Commutative, fixed-memory multiset digest, with both halves of each row hash.
    let digest:String=sqlx::query_scalar(r#"WITH hashes AS (
 SELECT md5(to_jsonb(m)::text) h FROM stats_m m UNION ALL SELECT md5(to_jsonb(t)::text) FROM stats_t t UNION ALL SELECT md5(to_jsonb(r)::text) FROM stats_r r
 ), sums AS (SELECT count(*) n,sum(('x'||substr(h,1,16))::bit(64)::bigint::numeric) a,
 sum(('x'||substr(h,17,16))::bit(64)::bigint::numeric) b FROM hashes)
 SELECT md5(concat_ws(':',n,a,b)) FROM sums"#).fetch_one(&mut **tx).await?;
    Ok(digest)
}
fn counts() -> String {
    "jsonb_build_object('total',count(*)::text,'sent',count(*) FILTER(WHERE outcome='sent')::text,'failed',count(*) FILTER(WHERE outcome='failed')::text,'unknown',count(*) FILTER(WHERE outcome='unknown')::text,'pending',count(*) FILTER(WHERE outcome='pending')::text)".into()
}
fn responsibilities() -> String {
    "jsonb_build_object('total',coalesce(sum(expected),0)::text,'sent',coalesce(sum(sent),0)::text,'failed',coalesce(sum(failed),0)::text,'unknown',coalesce(sum(unknown),0)::text,'pending',coalesce(sum(pending),0)::text)".into()
}
async fn overview(tx: &mut Tx<'_>, f: Filter, now: DateTime<Utc>, id: Uuid) -> Result<Overview> {
    let (start, end) = f.bounds();
    let summary:Summary=decode(sqlx::query_scalar::<_,Value>(&format!(r#"SELECT jsonb_build_object(
 'received',(SELECT count(*)::text FROM stats_m WHERE cohort_at >= $1 AND cohort_at<$2),
 'dispatched',(SELECT count(*)::text FROM stats_m m WHERE m.cohort_at >= $1 AND m.cohort_at<$2 AND EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id)),
 'not_dispatched',(SELECT count(*)::text FROM stats_m m WHERE m.cohort_at >= $1 AND m.cohort_at<$2 AND m.processing_terminal AND NOT EXISTS(SELECT 1 FROM statistics_targets t WHERE t.receipt_id=m.id)),
 'partial_channel_failure',(SELECT count(*)::text FROM stats_m m WHERE m.cohort_at >= $1 AND m.cohort_at<$2 AND m.channel_failure AND EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id)),
 'processing',(SELECT count(*)::text FROM stats_m m WHERE m.cohort_at >= $1 AND m.cohort_at<$2 AND (NOT m.scope_complete OR EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id AND t.outcome='pending'))),
 'incomplete',(SELECT count(*)::text FROM stats_m WHERE cohort_at >= $1 AND cohort_at<$2 AND NOT scope_complete),
 'historical_risk',count(*) FILTER(WHERE historical_risk)::text,
 'targets',{},'responsibilities',{},'attention',(SELECT {} FROM stats_t WHERE outcome IN ('pending','unknown')))
 FROM stats_t WHERE cohort_at >= $1 AND cohort_at<$2"#,counts(),responsibilities(),counts())).bind(start).bind(end).fetch_one(&mut **tx).await?)?;
    let mut cards = vec![];
    for p in periods(now) {
        let (count,previous):(String,String)=sqlx::query_as("SELECT count(*) FILTER(WHERE cohort_at >= $1 AND cohort_at<$2)::text,count(*) FILTER(WHERE cohort_at >= $3 AND cohort_at<$4)::text FROM stats_m")
 .bind(p.start).bind(p.end).bind(p.comparison_start).bind(p.comparison_end).fetch_one(&mut **tx).await?;
        cards.push(Card {
            key: p.key,
            start: p.start,
            end: p.end,
            comparison_start: p.comparison_start,
            comparison_end: p.comparison_end,
            count,
            previous,
        });
    }
    let trend:Vec<Bucket>=sqlx::query_scalar::<_,Value>(&format!(r#"WITH buckets AS (
 SELECT generate_series(date_trunc($3,$1::timestamptz AT TIME ZONE 'Asia/Shanghai'),
 date_trunc($3,($2::timestamptz-interval '1 microsecond') AT TIME ZONE 'Asia/Shanghai'),('1 '||$3)::interval) AS bucket),
 r AS (SELECT date_trunc($3,cohort_at AT TIME ZONE 'Asia/Shanghai') bucket,count(*)::text received FROM stats_m WHERE cohort_at >= $1 AND cohort_at<$2 GROUP BY 1),
 t AS (SELECT date_trunc($3,cohort_at AT TIME ZONE 'Asia/Shanghai') bucket,{} counts FROM stats_t WHERE cohort_at >= $1 AND cohort_at<$2 GROUP BY 1)
 SELECT jsonb_build_object('start',b.bucket AT TIME ZONE 'Asia/Shanghai','received',coalesce(r.received,'0'),
 'targets',coalesce(t.counts,'{{"total":"0","sent":"0","failed":"0","unknown":"0","pending":"0"}}'::jsonb))
 FROM buckets b LEFT JOIN r USING(bucket) LEFT JOIN t USING(bucket) ORDER BY b.bucket"#,counts())).bind(start).bind(end).bind(&f.grain).fetch_all(&mut **tx).await?.into_iter().map(decode).collect::<Result<_>>()?;
    let mut channels = breakdowns(tx, start, end, false).await?;
    for (channel, client) in [
        ("wecom", "wecom"),
        ("mobile", "wecom"),
        ("mobile", "wechat"),
    ] {
        if f.channel.as_deref().is_none_or(|v| v == channel)
            && f.client_kind.as_deref().is_none_or(|v| v == client)
        {
            let key = format!("{channel}:{client}");
            if !channels.iter().any(|g| g.id == key) {
                channels.push(Breakdown {
                    id: key.clone(),
                    name: key,
                    platform_name: None,
                    received: "0".into(),
                    targets: Counts::zero(),
                    responsibilities: Counts::zero(),
                });
            }
        }
    }
    channels.sort_by(|a, b| a.id.cmp(&b.id));
    for channel in &mut channels {
        channel.platform_name = None;
    }
    let units = breakdowns(tx, start, end, true).await?;
    let failures=sqlx::query_scalar::<_,Value>(r#"SELECT jsonb_build_object('object','target','reason_code',coalesce(reason_code,'unclassified'),'count',count(*)::text)
 FROM stats_t WHERE cohort_at >= $1 AND cohort_at<$2 AND outcome IN ('failed','unknown') GROUP BY reason_code
 UNION ALL SELECT jsonb_build_object('object','message','reason_code',coalesce(m.reason_code,'unclassified'),'count',count(*)::text)
 FROM stats_m m WHERE cohort_at >= $1 AND cohort_at<$2 AND processing_terminal AND NOT EXISTS(SELECT 1 FROM statistics_targets t WHERE t.receipt_id=m.id) GROUP BY reason_code"#)
 .bind(start).bind(end).fetch_all(&mut **tx).await?.into_iter().map(decode).collect::<Result<_>>()?;
    let unclassified:String=sqlx::query_scalar("SELECT count(*)::text FROM stats_t WHERE cohort_at >= $1 AND cohort_at<$2 AND client_kind='unclassified'").bind(start).bind(end).fetch_one(&mut **tx).await?;
    Ok(Overview {
        id,
        metric_version: METRIC_VERSION.into(),
        generated_at: now,
        expires_at: now + Duration::minutes(30),
        timezone: "Asia/Shanghai".into(),
        basis: "cohort".into(),
        filter: f,
        summary,
        cards,
        trend,
        channels,
        units,
        failures,
        unclassified,
    })
}
async fn breakdowns(
    tx: &mut Tx<'_>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    units: bool,
) -> Result<Vec<Breakdown>> {
    let key = if units {
        "coalesce(business_unit_id::text,'unassigned')"
    } else {
        "channel||':'||client_kind"
    };
    let name = if units {
        "min(unit_name)"
    } else {
        "channel||':'||client_kind"
    };
    let sql = format!(
        r#"WITH t AS (SELECT {key} id,{name} name,min(platform_name) platform_name,{} targets,{} responsibilities,
 count(DISTINCT receipt_id)::text received FROM stats_t WHERE cohort_at >= $1 AND cohort_at<$2 GROUP BY {key}),
 m AS (SELECT business_unit_id::text id,min(unit_name) name,min(platform_name) platform_name,count(*)::text received FROM stats_m WHERE cohort_at >= $1 AND cohort_at<$2 GROUP BY business_unit_id)
 SELECT jsonb_build_object('id',coalesce(t.id,m.id),'name',coalesce(t.name,m.name),'platform_name',coalesce(t.platform_name,m.platform_name),
 'received',{},'targets',coalesce(t.targets,'{{"total":"0","sent":"0","failed":"0","unknown":"0","pending":"0"}}'::jsonb),
 'responsibilities',coalesce(t.responsibilities,'{{"total":"0","sent":"0","failed":"0","unknown":"0","pending":"0"}}'::jsonb))
 FROM t FULL JOIN m ON {} WHERE {} ORDER BY coalesce((t.targets->>'total')::bigint,0) DESC,coalesce(t.id,m.id)"#,
        counts(),
        responsibilities(),
        if units {
            "coalesce(m.received,'0')"
        } else {
            "t.received"
        },
        if units { "t.id=m.id" } else { "false" },
        if units { "true" } else { "t.id IS NOT NULL" }
    );
    sqlx::query_scalar::<_, Value>(&sql)
        .bind(start)
        .bind(end)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(decode)
        .collect()
}
async fn load(tx: &mut Tx<'_>, actor: &Actor, id: Uuid) -> Result<Overview> {
    let row=sqlx::query("SELECT payload,fingerprint,expires_at FROM statistics_query_snapshots WHERE id=$1 AND actor_id=$2").bind(id).bind(actor.id).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    if row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now() {
        return Err(Error::Policy("statistics_snapshot_expired"));
    }
    let data: Overview = decode(row.get("payload"))?;
    let hash = prepare(tx, &data.filter, data.generated_at).await?;
    if hash != row.get::<String, _>("fingerprint") {
        return Err(Error::Policy("statistics_snapshot_changed"));
    }
    Ok(data)
}
impl StatisticsStore for Repository {
    async fn create(&self, actor: &Actor, filter: Filter) -> Result<Overview> {
        let _permit = QUERIES.try_acquire().map_err(|_| Error::Unavailable)?;
        let mut tx = begin(&self.0, actor).await?;
        let now = Utc::now();
        let id = Uuid::now_v7();
        let fingerprint = prepare(&mut tx, &filter, now).await?;
        let result = overview(&mut tx, filter, now, id).await?;
        sqlx::query("DELETE FROM statistics_query_snapshots WHERE id IN (SELECT id FROM statistics_query_snapshots WHERE expires_at<now() ORDER BY expires_at LIMIT 100 FOR UPDATE SKIP LOCKED)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO statistics_query_snapshots(id,actor_id,filter,fingerprint,payload,generated_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
 .bind(id).bind(actor.id).bind(json!(result.filter)).bind(fingerprint).bind(json!(result)).bind(now).bind(result.expires_at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn overview(&self, actor: &Actor, id: Uuid) -> Result<Overview> {
        let _permit = QUERIES.try_acquire().map_err(|_| Error::Unavailable)?;
        let mut tx = begin(&self.0, actor).await?;
        let data = load(&mut tx, actor, id).await?;
        tx.commit().await?;
        Ok(data)
    }
    async fn details(&self, actor: &Actor, id: Uuid, f: DetailFilter) -> Result<DetailPage> {
        let _permit = QUERIES.try_acquire().map_err(|_| Error::Unavailable)?;
        let mut tx = begin(&self.0, actor).await?;
        let data = load(&mut tx, actor, id).await?;
        let (start, end) = data.filter.bounds();
        let source = match f.object.as_str() {
            "message" => {
                r#"SELECT m.id::text,m.id receipt_id,NULL::uuid task_id,NULL::text target_name,m.unit_name,NULL::text channel,NULL::text client_kind,m.cohort_at,
 CASE WHEN m.processing_terminal AND NOT EXISTS(SELECT 1 FROM statistics_targets t WHERE t.receipt_id=m.id) THEN 'not_dispatched'
 WHEN NOT m.scope_complete THEN 'pending'
 WHEN EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id AND t.outcome='pending') THEN 'pending'
 WHEN EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id AND t.outcome='unknown') THEN 'unknown'
 WHEN m.channel_failure OR EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id AND t.outcome='failed') THEN CASE WHEN EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id AND t.outcome='sent') THEN 'partial_failed' ELSE 'failed' END
 WHEN EXISTS(SELECT 1 FROM stats_t t WHERE t.receipt_id=m.id) THEN 'sent' ELSE 'pending' END outcome,
 m.reason_code,NULL::uuid device_id,false historical_risk,m.business_unit_id FROM stats_m m"#
            }
            "responsibility" => "SELECT * FROM stats_r",
            _ => {
                "SELECT id,receipt_id,task_id,target_name,unit_name,channel,client_kind,cohort_at,outcome,reason_code,NULL::uuid device_id,historical_risk,business_unit_id FROM stats_t"
            }
        };
        let sql = format!(
            r#"SELECT to_jsonb(x)-'business_unit_id' FROM ({source}) x WHERE ($5 OR (cohort_at >= $1 AND cohort_at<$2))
 AND ($3::text IS NULL OR outcome=$3) AND ($4::text IS NULL OR coalesce(reason_code,'unclassified')=$4)
 AND (NOT $5 OR outcome IN ('pending','unknown'))
 AND ($6='' OR ($6='unassigned' AND business_unit_id IS NULL) OR business_unit_id::text=$6)
 AND ($8::timestamptz IS NULL OR date_trunc($9,cohort_at AT TIME ZONE 'Asia/Shanghai')=$8 AT TIME ZONE 'Asia/Shanghai')
 AND (NOT $10 OR outcome IN ('failed','unknown'))
 ORDER BY cohort_at DESC,id LIMIT 31 OFFSET $7"#
        );
        let mut items: Vec<Detail> = sqlx::query_scalar::<_, Value>(&sql)
            .bind(start)
            .bind(end)
            .bind(f.outcome)
            .bind(f.reason_code)
            .bind(f.attention)
            .bind(f.unit)
            .bind((f.page.max(1) - 1) * 30)
            .bind(f.bucket)
            .bind(&data.filter.grain)
            .bind(f.issues)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(decode)
            .collect::<Result<_>>()?;
        let has_more = items.len() > 30;
        items.truncate(30);
        tx.commit().await?;
        Ok(DetailPage {
            items,
            has_more,
            snapshot_id: id,
        })
    }
    async fn export(&self, actor: &Actor, id: Uuid) -> Result<Export> {
        let _permit = QUERIES.try_acquire().map_err(|_| Error::Unavailable)?;
        let mut tx = begin(&self.0, actor).await?;
        let data = load(&mut tx, actor, id).await?;
        let mut content = String::from("\u{feff}");
        let mut rows = 0;
        let mut write = |cells: Vec<String>| {
            content.push_str(
                &cells
                    .iter()
                    .map(|v| csv_cell(v))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            content.push_str("\r\n");
            rows += 1;
        };
        write(vec![
            "口径".into(),
            data.metric_version.clone(),
            data.timezone.clone(),
            data.generated_at.to_rfc3339(),
        ]);
        write(vec!["筛选".into(), json!(data.filter).to_string()]);
        write(vec![
            "观察方式".into(),
            data.basis.clone(),
            "范围未完整消息".into(),
            data.summary.incomplete.clone(),
        ]);
        write(vec![
            "期间接收".into(),
            data.summary.received.clone(),
            "未下发消息".into(),
            data.summary.not_dispatched.clone(),
            "部分通道未下发".into(),
            data.summary.partial_channel_failure.clone(),
        ]);
        write(vec![
            "接收周期".into(),
            "本期（条）".into(),
            "前期同进度（条）".into(),
            "本期开始".into(),
            "本期截止".into(),
        ]);
        for c in &data.cards {
            write(vec![
                c.key.clone(),
                c.count.clone(),
                c.previous.clone(),
                c.start.to_rfc3339(),
                c.end.to_rfc3339(),
            ]);
        }
        write(vec![
            "维度".into(),
            "标识".into(),
            "名称".into(),
            "来源平台".into(),
            "接收/涉及来源消息（条）".into(),
            "目标（项）".into(),
            "成功".into(),
            "失败".into(),
            "未知".into(),
            "处理中".into(),
            "应执行责任".into(),
        ]);
        for (kind, groups) in [("发送途径", &data.channels), ("业务单位", &data.units)] {
            for g in groups {
                write(vec![
                    kind.into(),
                    g.id.clone(),
                    match g.name.as_str() {
                        "wecom:wecom" => "企业微信官方通道",
                        "mobile:wecom" => "手机企业微信",
                        "mobile:wechat" => "手机个人微信",
                        "mobile:unclassified" => "手机类型未识别",
                        name => name,
                    }
                    .into(),
                    g.platform_name.clone().unwrap_or_default(),
                    g.received.clone(),
                    g.targets.total.clone(),
                    g.targets.sent.clone(),
                    g.targets.failed.clone(),
                    g.targets.unknown.clone(),
                    g.targets.pending.clone(),
                    g.responsibilities.total.clone(),
                ]);
            }
        }
        write(vec![
            "周期起点".into(),
            "接收（条）".into(),
            "目标（项）".into(),
            "成功".into(),
            "失败".into(),
            "未知".into(),
            "处理中".into(),
        ]);
        for b in &data.trend {
            write(vec![
                b.start.to_rfc3339(),
                b.received.clone(),
                b.targets.total.clone(),
                b.targets.sent.clone(),
                b.targets.failed.clone(),
                b.targets.unknown.clone(),
                b.targets.pending.clone(),
            ]);
        }
        crate::audit::south::adapter::postgres::record(
            &mut tx,
            actor,
            "message_statistics.export",
            id,
            json!({"filter":data.filter,"row_count":rows,"metric_version":METRIC_VERSION}),
        )
        .await?;
        tx.commit().await?;
        Ok(Export {
            filename: format!(
                "message-statistics-{}.csv",
                data.generated_at.format("%Y%m%d-%H%M%S")
            ),
            content,
            row_count: rows.to_string(),
        })
    }
}
