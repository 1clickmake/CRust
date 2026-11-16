use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Member {
    pub id: i64,
    pub mb_id: String,
    #[serde(skip_serializing)]
    pub mb_password: String,
    pub mb_name: String,
    pub mb_email: String,
    pub mb_level: i64,
    pub mb_point: i64,
    pub mb_today_login: Option<String>,
    pub mb_login_ip: Option<String>,
    pub is_blocked: i64,
    pub extra_data: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemberLevelConfig {
    pub level: i64,
    pub level_name: String,
    pub badge_icon: Option<String>,
    pub req_point: i64,
    pub req_posts: i64,
    pub req_comments: i64,
}
