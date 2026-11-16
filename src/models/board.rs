use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BoardGroup {
    pub gr_id: String,
    pub gr_subject: String,
    pub order_num: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Board {
    pub bo_table: String,
    pub gr_id: Option<String>,
    pub bo_subject: String,
    pub bo_category_list: Option<String>,
    pub bo_skin: String,
    pub bo_list_level: i64,
    pub bo_read_level: i64,
    pub bo_write_level: i64,
    pub bo_comment_level: i64,
    pub bo_download_level: i64,
    pub bo_read_point: i64,
    pub bo_write_point: i64,
    pub bo_comment_point: i64,
    pub bo_download_point: i64,
    pub bo_upload_count: i64,
    pub bo_upload_size: i64,
    pub extra_data: String,
    pub created_at: String,
}

impl Board {
    pub fn categories(&self) -> Vec<String> {
        self.bo_category_list
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}
