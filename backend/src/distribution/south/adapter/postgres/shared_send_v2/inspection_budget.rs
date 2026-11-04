//! One failure set for scheduling and both read projections.
use super::*;
#[derive(sqlx::FromRow)]
pub(super) struct Budget {
    pub used: i64,
    pub retry_at: Option<chrono::DateTime<chrono::Utc>>,
    pub budget_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl Budget {
    pub fn exhausted(&self, limit: i64) -> bool {
        self.used >= limit
    }
    pub fn next_retry(&self, limit: i64) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.exhausted(limit) {
            self.budget_at
        } else {
            self.retry_at
        }
    }
    pub fn waiting(&self, limit: i64) -> bool {
        self.next_retry(limit)
            .is_some_and(|at| at > chrono::Utc::now())
    }
}
pub(super) async fn load(
    tx: &mut Tx<'_>,
    binding: Uuid,
    dependencies: Value,
    delay: i64,
) -> Result<Budget> {
    Ok(sqlx::query_as("SELECT count(*) AS used,max(coalesce(finished_at,created_at))+make_interval(secs=>$3::double precision) AS retry_at,min(coalesce(finished_at,created_at))+interval '24 hours' AS budget_at FROM mobile_v9_send_inspections WHERE binding_id=$1 AND dependencies=$2 AND state='failed' AND (started_at IS NOT NULL OR result->'checks'->>'application_opened'='true') AND coalesce(finished_at,created_at)>clock_timestamp()-interval '24 hours'")
        .bind(binding).bind(dependencies).bind(delay as f64).fetch_one(&mut **tx).await?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhaustion_recovers_at_budget_expiry_instead_of_last_attempt_backoff() {
        let now = chrono::Utc::now();
        let b = Budget {
            used: 3,
            retry_at: Some(now - chrono::Duration::seconds(1)),
            budget_at: Some(now + chrono::Duration::hours(23)),
        };
        assert!(b.waiting(3));
        assert_eq!(b.next_retry(3), b.budget_at);
        assert!(!b.waiting(4));
        assert!(
            !Budget {
                used: 0,
                retry_at: None,
                budget_at: None
            }
            .waiting(3)
        );
    }
}
