use crate::error::AppError;
use crate::events::DomainEvent;
use crate::models::theme::{ThemeInfo, ThemeMeta};
use crate::state::{is_safe_slug, AppState};
use sqlx::SqlitePool;
use std::fs;
use std::path::Path;

pub async fn load_active_theme(pool: &SqlitePool) -> String {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT cf_value FROM site_configs WHERE cf_key = 'active_theme'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    match row {
        Some((t,)) if is_safe_slug(&t) && Path::new("templates/themes").join(&t).is_dir() => t,
        _ if Path::new("templates/themes/success").is_dir() => "success".to_string(),
        _ => "default".to_string(),
    }
}

pub fn list_available_themes(active_theme: &str) -> Vec<ThemeInfo> {
    let themes_dir = Path::new("templates/themes");
    let mut results = Vec::new();

    if let Ok(entries) = fs::read_dir(themes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if !is_safe_slug(&dir_name) {
                continue;
            }
            let toml_path = path.join("theme.toml");
            let meta = if toml_path.exists() {
                fs::read_to_string(&toml_path)
                    .ok()
                    .and_then(|s| toml::from_str::<ThemeMeta>(&s).ok())
                    .unwrap_or_else(|| default_meta(&dir_name))
            } else {
                default_meta(&dir_name)
            };

            results.push(ThemeInfo {
                is_active: dir_name == active_theme,
                meta,
                dir_name,
            });
        }
    }

    results.sort_by(|a, b| {
        if a.dir_name == "default" {
            std::cmp::Ordering::Less
        } else if b.dir_name == "default" {
            std::cmp::Ordering::Greater
        } else {
            a.dir_name.cmp(&b.dir_name)
        }
    });

    results
}

fn default_meta(slug: &str) -> ThemeMeta {
    ThemeMeta {
        name: slug.to_string(),
        slug: slug.to_string(),
        version: "1.0.0".to_string(),
        author: "Unknown".to_string(),
        description: "커스텀 디자인 테마".to_string(),
        thumbnail: None,
    }
}

/// Resolve a general template relative to `templates/`, falling back to `themes/default/{rel_path}`
pub fn resolve_template(theme: &str, rel_path: &str) -> String {
    let safe_theme = if is_safe_slug(theme) { theme } else { "default" };
    let clean_rel = rel_path.trim_start_matches('/');

    let active_candidate = Path::new("templates/themes").join(safe_theme).join(clean_rel);
    if active_candidate.exists() {
        return format!("themes/{}/{}", safe_theme, clean_rel);
    }

    format!("themes/default/{}", clean_rel)
}

/// 4-level fallback resolver for board skin templates
pub fn resolve_board_template(theme: &str, bo_skin: &str, file_name: &str) -> String {
    let safe_theme = if is_safe_slug(theme) { theme } else { "default" };
    let safe_skin = if is_safe_slug(bo_skin) { bo_skin } else { "basic" };

    let candidates = [
        format!("themes/{}/board/{}/{}", safe_theme, safe_skin, file_name),
        format!("themes/{}/board/basic/{}", safe_theme, file_name),
        format!("themes/default/board/{}/{}", safe_skin, file_name),
        format!("themes/default/board/basic/{}", file_name),
    ];

    for rel in &candidates {
        if Path::new("templates").join(rel).exists() {
            return rel.clone();
        }
    }

    candidates[3].clone()
}

pub async fn activate_theme(state: &AppState, new_theme: &str) -> Result<(), AppError> {
    if !is_safe_slug(new_theme) {
        return Err(AppError::BadRequest("유효하지 않은 테마 이름입니다.".into()));
    }
    let theme_dir = Path::new("templates/themes").join(new_theme);
    if !theme_dir.is_dir() {
        return Err(AppError::NotFound(format!("테마 '{}'을(를) 찾을 수 없습니다.", new_theme)));
    }

    sqlx::query(
        "INSERT INTO site_configs (cf_key, cf_value, description)
         VALUES ('active_theme', ?, '현재 활성화된 사이트 디자인 테마 폴더명')
         ON CONFLICT(cf_key) DO UPDATE SET cf_value = excluded.cf_value",
    )
    .bind(new_theme)
    .execute(&state.db)
    .await?;

    let old_theme = {
        let mut guard = state.active_theme.write().await;
        let old = guard.clone();
        *guard = new_theme.to_string();
        old
    };

    state.events.publish(DomainEvent::ThemeChanged {
        old_theme,
        new_theme: new_theme.to_string(),
    });

    Ok(())
}

/// Determine effective theme (supports admin preview query `?preview_theme=...`)
pub async fn effective_theme(
    state: &AppState,
    is_admin: bool,
    preview_param: Option<&str>,
) -> String {
    if is_admin {
        if let Some(p) = preview_param {
            if is_safe_slug(p) && Path::new("templates/themes").join(p).is_dir() {
                return p.to_string();
            }
        }
    }
    state.active_theme.read().await.clone()
}

/// Scan available board skins across default and active theme (`templates/themes/*/board/*`)
pub fn list_available_skins(active_theme: &str) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    set.insert("basic".to_string());
    for theme in ["default", active_theme] {
        if !is_safe_slug(theme) {
            continue;
        }
        let dir = Path::new("templates/themes").join(theme).join("board");
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if is_safe_slug(&name) {
                        set.insert(name);
                    }
                }
            }
        }
    }
    set.into_iter().collect()
}
