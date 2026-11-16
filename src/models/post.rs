use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Post {
    pub id: i64,
    pub bo_table: String,
    pub member_id: Option<i64>,
    pub wr_name: String,
    #[serde(skip_serializing)]
    pub wr_password: Option<String>,
    pub wr_category: Option<String>,
    pub wr_subject: String,
    pub wr_content: String,
    pub wr_hit: i64,
    pub wr_good: i64,
    pub wr_nogood: i64,
    pub wr_comment_count: i64,
    pub wr_file_count: i64,
    pub is_notice: i64,
    pub is_secret: i64,
    pub wr_link1: Option<String>,
    pub wr_link2: Option<String>,
    pub wr_ip: Option<String>,
    pub extra_data: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostListItem {
    #[serde(flatten)]
    pub post: Post,
    pub thumb: Option<String>,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PostFile {
    pub id: i64,
    pub file_uuid: String,
    pub post_id: Option<i64>,
    pub member_id: Option<i64>,
    pub file_type: String,
    pub mime_type: Option<String>,
    pub storage_driver: String,
    pub bf_source: String,
    pub bf_file: String,
    pub bf_path: String,
    pub bf_filesize: i64,
    pub bf_download_count: i64,
    pub is_temp: i64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: i64,
    pub post_id: i64,
    pub parent_id: Option<i64>,
    pub member_id: Option<i64>,
    pub co_name: String,
    #[serde(skip_serializing)]
    pub co_password: Option<String>,
    pub co_content: String,
    pub co_ip: Option<String>,
    pub is_secret: i64,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommentView {
    #[serde(flatten)]
    pub comment: Comment,
    pub depth: i64,
    pub can_view_secret: bool,
    pub can_manage: bool,
    pub is_guest_comment: bool,
}
