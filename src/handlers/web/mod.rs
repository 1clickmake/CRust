pub mod admin;
pub mod ai_builder;
pub mod board;
pub mod home;
pub mod member;
pub mod memo;
pub mod oauth;
pub mod seo;

use crate::error::AppError;
use crate::middlewares::auth::AuthContext;
use crate::models::board::Board;
use crate::services::{notification_service, settings_service, theme_service};
use crate::state::{is_safe_slug, AppState};
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use minijinja::context;
use serde::Deserialize;
use std::path::{Component, PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct PreviewQuery {
    pub preview_theme: Option<String>,
}

pub async fn build_base_context(
    state: &AppState,
    auth: &AuthContext,
    csrf_override: Option<String>,
    preview_theme: Option<&str>,
) -> Result<(String, minijinja::Value), AppError> {
    let theme = theme_service::effective_theme(state, auth.is_admin(), preview_theme).await;
    let is_preview = auth.is_admin()
        && preview_theme.is_some()
        && preview_theme != Some(state.active_theme.read().await.as_str());

    let settings = settings_service::load_all(&state.db).await;
    let site_title = settings.site_title.clone();

    let nav_boards = sqlx::query_as::<_, Board>(
        "SELECT * FROM boards ORDER BY bo_table ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let unread_noti = if let Some(ref m) = auth.member {
        notification_service::unread_count(&state.db, m.id).await.unwrap_or(0)
    } else {
        0
    };

    let csrf = csrf_override.unwrap_or_else(|| auth.csrf_token.clone());
    let layout_tpl = theme_service::resolve_template(&theme, "layout.html");
    let current_lang = crate::i18n::resolve_lang(
        auth.lang_cookie.as_deref(),
        &settings.default_lang,
        auth.accept_lang.as_deref(),
    );

    let val = context! {
        site_title => site_title,
        site_description => settings.site_description,
        base_url => auth.base_url,
        meta_robots => "index, follow",
        add_meta => settings.add_meta,
        analytics_script => settings.analytics_script,
        link_target => settings.link_target,
        current_theme => theme,
        current_lang => current_lang,
        supported_langs => crate::i18n::SUPPORTED_LANGS,
        layout_tpl => layout_tpl,
        is_preview => is_preview,
        current_user => auth.member,
        csrf_token => csrf,
        nav_boards => nav_boards,
        unread_noti => unread_noti,
    };

    Ok((theme, val))
}

/// Serve theme assets from `templates/themes/{theme}/assets/{*path}` with fallback to `default`
pub async fn serve_theme_asset(
    Path((theme_name, asset_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_safe_slug(&theme_name) {
        return Err(AppError::BadRequest("Invalid theme name".into()));
    }

    let mut clean_rel = PathBuf::new();
    for comp in std::path::Path::new(&asset_path).components() {
        match comp {
            Component::Normal(p) => clean_rel.push(p),
            _ => return Err(AppError::BadRequest("Invalid asset path".into())),
        }
    }

    let primary = std::path::Path::new("templates/themes")
        .join(&theme_name)
        .join("assets")
        .join(&clean_rel);
    let fallback = std::path::Path::new("templates/themes/default/assets").join(&clean_rel);

    let target = if primary.is_file() {
        primary
    } else if fallback.is_file() {
        fallback
    } else {
        return Err(AppError::NotFound("테마 에셋을 찾을 수 없습니다.".into()));
    };

    let bytes = tokio::fs::read(&target).await?;
    let mime = match target.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime)],
        bytes,
    ))
}
