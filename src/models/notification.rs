use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Notification {
    pub id: i64,
    pub member_id: i64,
    pub noti_type: String,
    pub message: String,
    pub target_url: Option<String>,
    pub is_read: i64,
    pub created_at: String,
}
