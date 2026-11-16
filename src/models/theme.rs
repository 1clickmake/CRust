use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeMeta {
    pub name: String,
    pub slug: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeInfo {
    pub meta: ThemeMeta,
    pub is_active: bool,
    pub dir_name: String,
}
