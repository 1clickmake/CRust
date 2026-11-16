pub mod providers;

use crate::error::AppError;
use crate::state::is_safe_slug;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub reasoning_effort: String,
    pub endpoint: String,
    pub allowed_paths: String,
    pub allow_db_config: bool,
    pub max_backups: i64,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: "gemini".to_string(),
            api_key: String::new(),
            model: "gemini-2.5-flash".to_string(),
            reasoning_effort: "default".to_string(),
            endpoint: String::new(),
            allowed_paths: "templates/themes/,locales/,templates/custom/".to_string(),
            allow_db_config: true,
            max_backups: 20,
        }
    }
}

pub fn normalize_reasoning_effort(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "minimal" => "minimal".to_string(),
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" => "high".to_string(),
        _ => "default".to_string(),
    }
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_plan(
        &self,
        client: &Client,
        cfg: &AiSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError>;
}

pub fn resolve_ai_provider(provider: &str) -> Box<dyn AiProvider> {
    match provider {
        "openai" => Box::new(providers::OpenAiProvider { compat_mode: false }),
        "openai_compat" => Box::new(providers::OpenAiProvider { compat_mode: true }),
        "claude" => Box::new(providers::ClaudeAiProvider),
        _ => Box::new(providers::GeminiAiProvider),
    }
}

pub async fn load_ai_settings(pool: &SqlitePool) -> AiSettings {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT cf_key, cf_value FROM site_configs WHERE cf_key LIKE 'ai_%'",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let map: HashMap<String, String> = rows.into_iter().collect();
    let d = AiSettings::default();
    let get = |k: &str, def: String| map.get(k).cloned().unwrap_or(def);

    AiSettings {
        provider: get("ai_provider", d.provider),
        api_key: get("ai_api_key", d.api_key),
        model: get("ai_model", d.model),
        reasoning_effort: normalize_reasoning_effort(&get("ai_reasoning_effort", d.reasoning_effort)),
        endpoint: get("ai_endpoint", d.endpoint),
        allowed_paths: get("ai_allowed_paths", d.allowed_paths),
        allow_db_config: map
            .get("ai_allow_db_config")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(d.allow_db_config),
        max_backups: map
            .get("ai_max_backups")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(d.max_backups)
            .clamp(1, 100),
    }
}

pub async fn save_ai_settings(pool: &SqlitePool, s: &AiSettings) -> Result<(), AppError> {
    let pairs = [
        ("ai_provider", s.provider.trim().to_string()),
        ("ai_api_key", s.api_key.trim().to_string()),
        ("ai_model", s.model.trim().to_string()),
        (
            "ai_reasoning_effort",
            normalize_reasoning_effort(&s.reasoning_effort),
        ),
        ("ai_endpoint", s.endpoint.trim().to_string()),
        ("ai_allowed_paths", s.allowed_paths.trim().to_string()),
        (
            "ai_allow_db_config",
            if s.allow_db_config { "1" } else { "0" }.to_string(),
        ),
        ("ai_max_backups", s.max_backups.clamp(1, 100).to_string()),
    ];
    let mut tx = pool.begin().await?;
    for (k, v) in pairs {
        sqlx::query(
            "INSERT INTO site_configs (cf_key, cf_value) VALUES (?, ?)
             ON CONFLICT(cf_key) DO UPDATE SET cf_value = excluded.cf_value",
        )
        .bind(k)
        .bind(v)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AiAction {
    #[serde(rename = "write_file")]
    WriteFile { path: String, content: String },
    #[serde(rename = "upsert_page")]
    UpsertPage {
        slug: String,
        title: String,
        content_html: String,
    },
    #[serde(rename = "set_config")]
    SetConfig { cf_key: String, cf_value: String },
    #[serde(rename = "update_board")]
    UpdateBoard {
        bo_table: String,
        bo_skin: Option<String>,
        bo_subject: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlan {
    pub summary: String,
    pub actions: Vec<AiAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiBackupRow {
    pub id: i64,
    pub admin_id: i64,
    pub prompt_summary: String,
    pub provider_used: String,
    pub actions_json: String,
    pub status: String,
    pub created_at: String,
    pub rolled_back_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CustomPage {
    pub slug: String,
    pub title: String,
    pub content_html: String,
    pub is_published: i64,
    pub created_at: String,
    pub updated_at: String,
}

const ALLOWED_FILE_EXTENSIONS: &[&str] = &["html", "css", "js", "toml", "json", "txt", "md"];

pub fn validate_whitelisted_path(raw_path: &str, allowed_prefixes_csv: &str) -> Result<String, AppError> {
    let norm = raw_path.trim().replace('\\', "/");
    if norm.is_empty() || norm.starts_with('/') || norm.contains(':') {
        return Err(AppError::Forbidden(format!(
            "허용되지 않은 파일 경로입니다: {}",
            raw_path
        )));
    }
    for comp in Path::new(&norm).components() {
        if !matches!(comp, Component::Normal(_)) {
            return Err(AppError::Forbidden(format!(
                "경로 탐색(Traversal)이 차단되었습니다: {}",
                raw_path
            )));
        }
    }

    let ext = Path::new(&norm)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !ALLOWED_FILE_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::Forbidden(format!(
            "허용되지 않은 확장자(.{})입니다. HTML/CSS/JS/TOML/JSON/TXT/MD만 허용됩니다.",
            ext
        )));
    }

    let prefixes: Vec<String> = allowed_prefixes_csv
        .split(',')
        .map(|s| s.trim().replace('\\', "/"))
        .filter(|s| !s.is_empty())
        .collect();

    if !prefixes.iter().any(|p| norm.starts_with(p.as_str())) {
        return Err(AppError::Forbidden(format!(
            "화이트리스트({}) 외부 경로 쓰기가 차단되었습니다: {}",
            allowed_prefixes_csv, norm
        )));
    }

    Ok(norm)
}

pub async fn generate_ai_plan(pool: &SqlitePool, user_prompt: &str) -> Result<String, AppError> {
    let clean = user_prompt.trim();
    if clean.is_empty() {
        return Err(AppError::BadRequest("프롬프트를 입력해 주세요.".into()));
    }
    let cfg = load_ai_settings(pool).await;
    let provider = resolve_ai_provider(&cfg.provider);
    let system_prompt = format!(
        "You are CRust CMS AI Builder. Output ONLY valid JSON matching:\n\
         {{\"summary\":\"short summary\",\"actions\":[...]}}\n\
         Allowed actions:\n\
         1. {{\"type\":\"write_file\",\"path\":\"templates/themes/<name>/...\",\"content\":\"...\"}}\n\
         2. {{\"type\":\"upsert_page\",\"slug\":\"about\",\"title\":\"...\",\"content_html\":\"...\"}}\n\
         3. {{\"type\":\"set_config\",\"cf_key\":\"site_title\",\"cf_value\":\"...\"}}\n\
         4. {{\"type\":\"update_board\",\"bo_table\":\"free\",\"bo_skin\":\"gallery\",\"bo_subject\":\"...\"}}\n\
         Strict allowed path prefixes: {}.\n\
         DB config changes allowed: {}.",
        cfg.allowed_paths, cfg.allow_db_config
    );

    let client = Client::new();
    let raw = provider
        .generate_plan(&client, &cfg, &system_prompt, clean)
        .await?;
    // Validate JSON parses as AiPlan
    let parsed: AiPlan = serde_json::from_str(&raw).map_err(|e| {
        AppError::BadRequest(format!("AI 응답이 AiPlan JSON 규격과 일치하지 않습니다: {}", e))
    })?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or(raw))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum SnapshotItem {
    #[serde(rename = "file")]
    File {
        path: String,
        existed: bool,
        prev_content: Option<String>,
    },
    #[serde(rename = "page")]
    Page {
        slug: String,
        existed: bool,
        prev_title: Option<String>,
        prev_html: Option<String>,
        prev_published: Option<i64>,
    },
    #[serde(rename = "config")]
    Config {
        cf_key: String,
        existed: bool,
        prev_value: Option<String>,
    },
    #[serde(rename = "board")]
    Board {
        bo_table: String,
        prev_skin: String,
        prev_subject: String,
    },
}

/// Validate all actions upfront, capture pre-change snapshot, execute safely, and record backup ID.
pub async fn apply_plan_with_snapshot(
    pool: &SqlitePool,
    admin_id: i64,
    plan_json: &str,
) -> Result<i64, AppError> {
    let cfg = load_ai_settings(pool).await;
    let plan: AiPlan = serde_json::from_str(plan_json)
        .map_err(|e| AppError::BadRequest(format!("유효하지 않은 실행 계획 JSON입니다: {}", e)))?;

    if plan.actions.is_empty() {
        return Err(AppError::BadRequest("실행할 액션이 없습니다.".into()));
    }

    // 1. Strict upfront validation BEFORE mutating anything
    for act in &plan.actions {
        match act {
            AiAction::WriteFile { path, .. } => {
                validate_whitelisted_path(path, &cfg.allowed_paths)?;
            }
            AiAction::UpsertPage { slug, .. } => {
                if !is_safe_slug(slug) {
                    return Err(AppError::BadRequest(format!("잘못된 페이지 슬러그입니다: {}", slug)));
                }
            }
            AiAction::SetConfig { cf_key, .. } => {
                if !cfg.allow_db_config {
                    return Err(AppError::Forbidden("안전 정책상 DB 설정 변경이 비활성화되어 있습니다.".into()));
                }
                if cf_key.starts_with("ai_") {
                    return Err(AppError::Forbidden("AI 안전 정책 키 자체는 빌더로 변경할 수 없습니다.".into()));
                }
            }
            AiAction::UpdateBoard { bo_table, .. } => {
                if !cfg.allow_db_config {
                    return Err(AppError::Forbidden("안전 정책상 게시판 설정 변경이 비활성화되어 있습니다.".into()));
                }
                let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM boards WHERE bo_table = ?")
                    .bind(bo_table.trim())
                    .fetch_one(pool)
                    .await?;
                if exists.0 == 0 {
                    return Err(AppError::NotFound(format!("대상 게시판({})을 찾을 수 없습니다.", bo_table)));
                }
            }
        }
    }

    // 2. Capture full pre-change snapshot
    let mut snapshots: Vec<SnapshotItem> = Vec::new();
    for act in &plan.actions {
        match act {
            AiAction::WriteFile { path, .. } => {
                let norm = validate_whitelisted_path(path, &cfg.allowed_paths)?;
                let pb = PathBuf::from(&norm);
                if pb.exists() {
                    let prev = tokio::fs::read_to_string(&pb).await.ok();
                    snapshots.push(SnapshotItem::File {
                        path: norm,
                        existed: true,
                        prev_content: prev,
                    });
                } else {
                    snapshots.push(SnapshotItem::File {
                        path: norm,
                        existed: false,
                        prev_content: None,
                    });
                }
            }
            AiAction::UpsertPage { slug, .. } => {
                let prev: Option<(String, String, i64)> = sqlx::query_as(
                    "SELECT title, content_html, is_published FROM custom_pages WHERE slug = ?",
                )
                .bind(slug)
                .fetch_optional(pool)
                .await?;
                if let Some((t, h, p)) = prev {
                    snapshots.push(SnapshotItem::Page {
                        slug: slug.clone(),
                        existed: true,
                        prev_title: Some(t),
                        prev_html: Some(h),
                        prev_published: Some(p),
                    });
                } else {
                    snapshots.push(SnapshotItem::Page {
                        slug: slug.clone(),
                        existed: false,
                        prev_title: None,
                        prev_html: None,
                        prev_published: None,
                    });
                }
            }
            AiAction::SetConfig { cf_key, .. } => {
                let prev: Option<(String,)> =
                    sqlx::query_as("SELECT cf_value FROM site_configs WHERE cf_key = ?")
                        .bind(cf_key)
                        .fetch_optional(pool)
                        .await?;
                snapshots.push(SnapshotItem::Config {
                    cf_key: cf_key.clone(),
                    existed: prev.is_some(),
                    prev_value: prev.map(|r| r.0),
                });
            }
            AiAction::UpdateBoard { bo_table, .. } => {
                let prev: Option<(String, String)> =
                    sqlx::query_as("SELECT bo_skin, bo_subject FROM boards WHERE bo_table = ?")
                        .bind(bo_table)
                        .fetch_optional(pool)
                        .await?;
                if let Some((sk, sb)) = prev {
                    snapshots.push(SnapshotItem::Board {
                        bo_table: bo_table.clone(),
                        prev_skin: sk,
                        prev_subject: sb,
                    });
                }
            }
        }
    }

    // 3. Apply mutations
    for act in &plan.actions {
        match act {
            AiAction::WriteFile { path, content } => {
                let norm = validate_whitelisted_path(path, &cfg.allowed_paths)?;
                let pb = PathBuf::from(&norm);
                if let Some(parent) = pb.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&pb, content.as_bytes()).await?;
            }
            AiAction::UpsertPage {
                slug,
                title,
                content_html,
            } => {
                sqlx::query(
                    "INSERT INTO custom_pages (slug, title, content_html, is_published, updated_at)
                     VALUES (?, ?, ?, 1, CURRENT_TIMESTAMP)
                     ON CONFLICT(slug) DO UPDATE SET title = excluded.title, content_html = excluded.content_html, is_published = 1, updated_at = CURRENT_TIMESTAMP",
                )
                .bind(slug.trim())
                .bind(title.trim())
                .bind(content_html)
                .execute(pool)
                .await?;
            }
            AiAction::SetConfig { cf_key, cf_value } => {
                sqlx::query(
                    "INSERT INTO site_configs (cf_key, cf_value) VALUES (?, ?)
                     ON CONFLICT(cf_key) DO UPDATE SET cf_value = excluded.cf_value",
                )
                .bind(cf_key.trim())
                .bind(cf_value)
                .execute(pool)
                .await?;
            }
            AiAction::UpdateBoard {
                bo_table,
                bo_skin,
                bo_subject,
            } => {
                if let Some(ref sk) = bo_skin {
                    if is_safe_slug(sk) {
                        sqlx::query("UPDATE boards SET bo_skin = ? WHERE bo_table = ?")
                            .bind(sk.trim())
                            .bind(bo_table.trim())
                            .execute(pool)
                            .await?;
                    }
                }
                if let Some(ref sb) = bo_subject {
                    sqlx::query("UPDATE boards SET bo_subject = ? WHERE bo_table = ?")
                        .bind(sb.trim())
                        .bind(bo_table.trim())
                        .execute(pool)
                        .await?;
                }
            }
        }
    }

    // 4. Record backup snapshot & trim excess backups
    let snap_json = serde_json::to_string(&snapshots).unwrap_or_else(|_| "[]".to_string());
    let summary = if plan.summary.trim().is_empty() {
        "AI Builder Execution".to_string()
    } else {
        plan.summary.chars().take(240).collect()
    };

    let res = sqlx::query(
        "INSERT INTO ai_change_backups (admin_id, prompt_summary, provider_used, actions_json, snapshot_json, status)
         VALUES (?, ?, ?, ?, ?, 'applied')",
    )
    .bind(admin_id)
    .bind(summary)
    .bind(&cfg.provider)
    .bind(json!(plan).to_string())
    .bind(snap_json)
    .execute(pool)
    .await?;

    let keep = cfg.max_backups.max(1);
    sqlx::query(
        "DELETE FROM ai_change_backups WHERE id NOT IN (SELECT id FROM ai_change_backups ORDER BY id DESC LIMIT ?)",
    )
    .bind(keep)
    .execute(pool)
    .await?;

    Ok(res.last_insert_rowid())
}

pub async fn rollback_backup(pool: &SqlitePool, backup_id: i64) -> Result<(), AppError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT snapshot_json, status FROM ai_change_backups WHERE id = ?",
    )
    .bind(backup_id)
    .fetch_optional(pool)
    .await?;

    let (snap_json, status) =
        row.ok_or_else(|| AppError::NotFound("백업 스냅샷을 찾을 수 없습니다.".into()))?;
    if status == "rolled_back" {
        return Err(AppError::BadRequest("이미 롤백된 스냅샷입니다.".into()));
    }

    let items: Vec<SnapshotItem> = serde_json::from_str(&snap_json)
        .map_err(|e| AppError::Internal(format!("스냅샷 파싱 오류: {}", e)))?;

    // Reverse order so sequential edits unwind cleanly
    for item in items.into_iter().rev() {
        match item {
            SnapshotItem::File {
                path,
                existed,
                prev_content,
            } => {
                let pb = PathBuf::from(&path);
                if existed {
                    if let Some(prev) = prev_content {
                        if let Some(parent) = pb.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        tokio::fs::write(&pb, prev.as_bytes()).await?;
                    }
                } else if pb.exists() {
                    let _ = tokio::fs::remove_file(&pb).await;
                    if let Some(parent) = pb.parent() {
                        let _ = tokio::fs::remove_dir(parent).await; // removes empty newly-created dir only
                    }
                }
            }
            SnapshotItem::Page {
                slug,
                existed,
                prev_title,
                prev_html,
                prev_published,
            } => {
                if existed {
                    sqlx::query(
                        "UPDATE custom_pages SET title = ?, content_html = ?, is_published = ?, updated_at = CURRENT_TIMESTAMP WHERE slug = ?",
                    )
                    .bind(prev_title.unwrap_or_default())
                    .bind(prev_html.unwrap_or_default())
                    .bind(prev_published.unwrap_or(1))
                    .bind(&slug)
                    .execute(pool)
                    .await?;
                } else {
                    sqlx::query("DELETE FROM custom_pages WHERE slug = ?")
                        .bind(&slug)
                        .execute(pool)
                        .await?;
                }
            }
            SnapshotItem::Config {
                cf_key,
                existed,
                prev_value,
            } => {
                if existed {
                    sqlx::query("UPDATE site_configs SET cf_value = ? WHERE cf_key = ?")
                        .bind(prev_value.unwrap_or_default())
                        .bind(&cf_key)
                        .execute(pool)
                        .await?;
                } else {
                    sqlx::query("DELETE FROM site_configs WHERE cf_key = ?")
                        .bind(&cf_key)
                        .execute(pool)
                        .await?;
                }
            }
            SnapshotItem::Board {
                bo_table,
                prev_skin,
                prev_subject,
            } => {
                sqlx::query("UPDATE boards SET bo_skin = ?, bo_subject = ? WHERE bo_table = ?")
                    .bind(prev_skin)
                    .bind(prev_subject)
                    .bind(&bo_table)
                    .execute(pool)
                    .await?;
            }
        }
    }

    sqlx::query(
        "UPDATE ai_change_backups SET status = 'rolled_back', rolled_back_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(backup_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_backups(pool: &SqlitePool) -> Result<Vec<AiBackupRow>, AppError> {
    let rows = sqlx::query_as::<_, AiBackupRow>(
        "SELECT id, admin_id, prompt_summary, provider_used, actions_json, status, created_at, rolled_back_at
         FROM ai_change_backups ORDER BY id DESC LIMIT 50",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_published_page(pool: &SqlitePool, slug: &str) -> Result<CustomPage, AppError> {
    sqlx::query_as::<_, CustomPage>(
        "SELECT * FROM custom_pages WHERE slug = ? AND is_published = 1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("요청하신 페이지를 찾을 수 없습니다.".into()))
}

#[cfg(test)]
mod tests {
    use super::{normalize_reasoning_effort, validate_whitelisted_path};

    #[test]
    fn test_ai_path_whitelist_blocks_traversal_and_code() {
        let allowed = "templates/themes/,locales/,templates/custom/";
        assert!(validate_whitelisted_path("templates/themes/neon/theme.toml", allowed).is_ok());
        assert!(validate_whitelisted_path("locales/ko.json", allowed).is_ok());
        assert!(validate_whitelisted_path("src/main.rs", allowed).is_err());
        assert!(validate_whitelisted_path("templates/themes/../../secret.txt", allowed).is_err());
        assert!(validate_whitelisted_path("C:\\Windows\\win.ini", allowed).is_err());
        assert!(validate_whitelisted_path("templates/themes/evil.rs", allowed).is_err());
    }

    #[test]
    fn test_normalize_reasoning_effort() {
        assert_eq!(normalize_reasoning_effort("minimal"), "minimal");
        assert_eq!(normalize_reasoning_effort(" LOW "), "low");
        assert_eq!(normalize_reasoning_effort("Medium"), "medium");
        assert_eq!(normalize_reasoning_effort("HIGH"), "high");
        assert_eq!(normalize_reasoning_effort("xhigh"), "default");
        assert_eq!(normalize_reasoning_effort(""), "default");
    }
}
