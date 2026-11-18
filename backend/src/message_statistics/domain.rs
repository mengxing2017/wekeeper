use crate::business::{Result, require};
use chrono::{DateTime, Datelike, Duration, FixedOffset, Months, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const METRIC_VERSION: &str = "message-statistics/1";
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub grain: String,
    pub platform_id: Option<Uuid>,
    pub business_unit_id: Option<Uuid>,
    pub route_id: Option<String>,
    pub source_kind: String,
    pub channel: Option<String>,
    pub client_kind: Option<String>,
}
impl Filter {
    pub fn normalize(mut self, now: DateTime<Utc>) -> Result<Self> {
        let today = (now + Duration::hours(8)).date_naive();
        self.start.get_or_insert(today - Duration::days(29));
        self.end.get_or_insert(today);
        if self.grain.is_empty() {
            self.grain = "day".into();
        }
        if self.source_kind.is_empty() {
            self.source_kind = "all".into();
        }
        require(
            matches!(
                self.grain.as_str(),
                "hour" | "day" | "week" | "month" | "year"
            ),
            "统计粒度不合法",
        )?;
        require(
            matches!(self.source_kind.as_str(), "all" | "manual" | "source_route"),
            "消息来源不合法",
        )?;
        require(
            self.channel
                .as_deref()
                .is_none_or(|v| matches!(v, "wecom" | "mobile")),
            "通道不合法",
        )?;
        require(
            self.client_kind
                .as_deref()
                .is_none_or(|v| matches!(v, "wecom" | "wechat" | "unclassified")),
            "客户端类型不合法",
        )?;
        require(
            !(self.channel.as_deref() == Some("wecom")
                && self.client_kind.as_deref().is_some_and(|v| v != "wecom")),
            "官方通道仅支持企业微信",
        )?;
        require(
            !(self.source_kind == "manual" && self.route_id.is_some()),
            "手动任务不属于来源路由",
        )?;
        let start = self.start.unwrap();
        let end = self.end.unwrap();
        require(
            start <= end && end <= today && start.year() >= 2000,
            "统计日期范围不合法",
        )?;
        let days = (end - start).num_days() + 1;
        require(
            days <= 36600
                && (self.grain != "hour" || days <= 31)
                && (self.grain != "day" || days <= 3660),
            "时间范围过大，请调整统计粒度",
        )?;
        require(
            self.route_id
                .as_ref()
                .is_none_or(|s| !s.trim().is_empty() && s.len() <= 200),
            "路由不合法",
        )?;
        Ok(self)
    }
    pub fn bounds(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        (
            midnight(self.start.unwrap()),
            midnight(self.end.unwrap().succ_opt().unwrap()),
        )
    }
}
pub fn midnight(date: NaiveDate) -> DateTime<Utc> {
    FixedOffset::east_opt(8 * 3600)
        .unwrap()
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Period {
    pub key: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub comparison_start: DateTime<Utc>,
    pub comparison_end: DateTime<Utc>,
}
pub fn periods(now: DateTime<Utc>) -> Vec<Period> {
    let local = now + Duration::hours(8);
    let date = local.date_naive();
    let starts = [
        ("day", date),
        (
            "week",
            date - Duration::days(date.weekday().num_days_from_monday().into()),
        ),
        ("month", date.with_day(1).unwrap()),
        ("year", NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap()),
    ];
    starts
        .into_iter()
        .map(|(key, start)| {
            let previous = match key {
                "day" => start - Duration::days(1),
                "week" => start - Duration::weeks(1),
                "month" => start.checked_sub_months(Months::new(1)).unwrap(),
                _ => start.checked_sub_months(Months::new(12)).unwrap(),
            };
            let comparison_end = match key {
                "day" => now - Duration::days(1),
                "week" => now - Duration::weeks(1),
                _ => {
                    let day = date
                        .checked_sub_months(Months::new(if key == "month" { 1 } else { 12 }))
                        .unwrap();
                    midnight(day)
                        + Duration::seconds(local.time().num_seconds_from_midnight().into())
                }
            };
            Period {
                key: key.into(),
                start: midnight(start),
                end: now,
                comparison_start: midnight(previous),
                comparison_end,
            }
        })
        .collect()
}
use chrono::Timelike;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub total: String,
    pub sent: String,
    pub failed: String,
    pub unknown: String,
    pub pending: String,
}
impl Counts {
    pub fn zero() -> Self {
        Self {
            total: "0".into(),
            sent: "0".into(),
            failed: "0".into(),
            unknown: "0".into(),
            pending: "0".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub key: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub comparison_start: DateTime<Utc>,
    pub comparison_end: DateTime<Utc>,
    pub count: String,
    pub previous: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bucket {
    pub start: DateTime<Utc>,
    pub received: String,
    pub targets: Counts,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Breakdown {
    pub id: String,
    pub name: String,
    pub platform_name: Option<String>,
    pub received: String,
    pub targets: Counts,
    pub responsibilities: Counts,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub received: String,
    pub dispatched: String,
    pub not_dispatched: String,
    pub partial_channel_failure: String,
    pub processing: String,
    pub incomplete: String,
    pub historical_risk: String,
    pub targets: Counts,
    pub responsibilities: Counts,
    pub attention: Counts,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Overview {
    pub id: Uuid,
    pub metric_version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub timezone: String,
    pub basis: String,
    pub filter: Filter,
    pub summary: Summary,
    pub cards: Vec<Card>,
    pub trend: Vec<Bucket>,
    pub channels: Vec<Breakdown>,
    pub units: Vec<Breakdown>,
    pub failures: Vec<Failure>,
    pub unclassified: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Failure {
    pub object: String,
    pub reason_code: String,
    pub count: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DetailFilter {
    pub bucket: Option<DateTime<Utc>>,
    pub issues: bool,
    pub object: String,
    pub outcome: Option<String>,
    pub reason_code: Option<String>,
    pub unit: String,
    pub attention: bool,
    pub page: i64,
}
impl DetailFilter {
    pub fn validate(&self) -> Result<()> {
        require(
            matches!(
                self.object.as_str(),
                "" | "message" | "target" | "responsibility"
            ),
            "统计对象不合法",
        )?;
        require(
            self.outcome.as_deref().is_none_or(|v| {
                matches!(
                    v,
                    "sent" | "failed" | "unknown" | "pending" | "not_dispatched"
                )
            }),
            "结果不合法",
        )?;
        require((0..=100000).contains(&self.page), "页码不合法")?;
        require(
            self.unit.is_empty() || self.unit == "unassigned" || self.unit.parse::<Uuid>().is_ok(),
            "单位不合法",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub id: String,
    pub receipt_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub target_name: Option<String>,
    pub unit_name: String,
    pub channel: Option<String>,
    pub client_kind: Option<String>,
    pub cohort_at: DateTime<Utc>,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub device_id: Option<Uuid>,
    pub historical_risk: bool,
}
#[derive(Serialize, Deserialize)]
pub struct DetailPage {
    pub items: Vec<Detail>,
    pub has_more: bool,
    pub snapshot_id: Uuid,
}
#[derive(Serialize, Deserialize)]
pub struct Export {
    pub filename: String,
    pub content: String,
    pub row_count: String,
}

pub fn csv_cell(value: &str) -> String {
    let value: String = value
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect();
    let formula = value.trim_start().starts_with(['=', '+', '-', '@'])
        || value.starts_with(['\t', '\r', '\n']);
    format!(
        "\"{}{}\"",
        if formula { "'" } else { "" },
        value.replace('"', "\"\"")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn periods_use_shanghai_and_clamp_short_months() {
        let now = "2024-03-31T10:00:00Z".parse().unwrap();
        let p = periods(now);
        assert_eq!(
            p[2].comparison_end.to_rfc3339(),
            "2024-02-29T10:00:00+00:00"
        );
        assert_eq!(p[1].start.to_rfc3339(), "2024-03-24T16:00:00+00:00");
    }
    #[test]
    fn rejects_conflicting_channel_and_oversized_hour_range() {
        let now = Utc::now();
        assert!(
            Filter {
                channel: Some("wecom".into()),
                client_kind: Some("wechat".into()),
                ..Default::default()
            }
            .normalize(now)
            .is_err()
        );
        assert!(
            Filter {
                start: Some((now - Duration::days(100)).date_naive()),
                grain: "hour".into(),
                ..Default::default()
            }
            .normalize(now)
            .is_err()
        );
    }
    #[test]
    fn csv_escapes_formula_and_quotes() {
        assert_eq!(csv_cell(" =1+1"), "\"' =1+1\"");
        assert_eq!(csv_cell("a,\"b"), "\"a,\"\"b\"");
    }
    #[test]
    fn midnight_rolls_to_previous_utc_day() {
        assert_eq!(
            midnight(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).to_rfc3339(),
            "2025-12-31T16:00:00+00:00"
        );
        assert!(
            NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .checked_add_days(chrono::Days::new(1))
                .is_some()
        );
    }
}
