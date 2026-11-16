use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PointLog {
    pub id: i64,
    pub member_id: i64,
    pub rel_table: Option<String>,
    pub rel_id: Option<String>,
    pub rel_action: String,
    pub point: i64,
    pub balance_after: i64,
    pub content: String,
    pub created_at: String,
}
