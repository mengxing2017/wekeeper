//! Platform policy is distinct from source binding and from channel delivery evidence.
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePolicy {
    pub namespace: String,
    pub enabled: bool,
    pub max_pending: i32,
    pub revision: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicy {
    pub resource_key: String,
    pub enabled: bool,
    pub window_seconds: i32,
    pub max_calls: i32,
    pub revision: i64,
}
impl ResourcePolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            self.resource_key.starts_with("wecom:") && self.resource_key.len() <= 500,
            "官方资源键无效",
        )?;
        require(
            (1..=86400 * 30).contains(&self.window_seconds)
                && (1..=100000).contains(&self.max_calls),
            "资源窗口或额度无效",
        )?;
        require(self.revision >= 0, "政策版本无效")
    }
}
