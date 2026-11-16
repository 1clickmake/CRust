mod config;
mod db;
mod error;
mod events;
mod handlers;
mod i18n;
mod middlewares;
mod models;
mod services;
mod state;
mod storage;

use crate::config::AppConfig;
use crate::events::EventBus;
use crate::handlers::{api, web};
use crate::services::{file_service, theme_service};
use crate::state::AppState;
use crate::storage::build_storage_backend;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Arc::new(AppConfig::load("config.toml")?);

    tokio::fs::create_dir_all(format!("{}/editor", cfg.storage.upload_dir)).await?;
    tokio::fs::create_dir_all(format!("{}/board", cfg.storage.upload_dir)).await?;

    let pool = db::init_db(&cfg.database).await?;
    let active_theme_name = theme_service::load_active_theme(&pool).await;

    let storage = build_storage_backend(&cfg.storage);
    let events = Arc::new(EventBus::new(256));
    let i18n_data = Arc::new(i18n::load_translations());
    let jinja = Arc::new(AppState::build_jinja_env(i18n_data.clone()));

    let state = AppState {
        db: pool,
        config: cfg.clone(),
        active_theme: Arc::new(RwLock::new(active_theme_name.clone())),
        storage,
        events: events.clone(),
        jinja,
        i18n: i18n_data,
    };

    // Background EventBus listener (extensible hook dispatcher)
    events::listener::spawn_event_listener(state.clone());

    // Background Orphan Temp File Garbage Collector (runs every 6 hours)
    let gc_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        loop {
            interval.tick().await;
            if let Ok(cleaned) = file_service::cleanup_orphan_temp_files(&gc_state).await {
                if cleaned > 0 {
                    tracing::info!("[OrphanGC] 미사용 임시 파일 {}건을 정리했습니다.", cleaned);
                }
            }
        }
    });

    let max_body = (cfg.storage.max_file_size_mb as usize) * 1024 * 1024 + 1024 * 1024;

    let app = Router::new()
        // Home & i18n & SEO
        .route("/", get(web::home::index))
        .route("/lang/{locale}", get(web::home::set_language))
        .route("/robots.txt", get(web::seo::robots_txt))
        .route("/sitemap.xml", get(web::seo::sitemap_xml))
        .route("/rss", get(web::seo::rss_feed))
        .route("/page/{slug}", get(web::ai_builder::custom_page_view))
        // Member
        .route("/member/login", get(web::member::login_page).post(web::member::login_submit))
        .route("/member/register", get(web::member::register_page).post(web::member::register_submit))
        .route("/member/logout", post(web::member::logout))
        .route("/member/profile", get(web::member::profile_page))
        .route("/member/profile/update", post(web::member::profile_update))
        .route("/member/profile/password", post(web::member::profile_password_update))
        .route("/member/memo", get(web::memo::memo_page))
        .route("/member/memo/send", post(web::memo::memo_send))
        .route("/member/memo/{id}/delete", post(web::memo::memo_delete))
        // Social OAuth2
        .route("/auth/oauth/{provider}", get(web::oauth::oauth_start))
        .route("/auth/oauth/{provider}/callback", get(web::oauth::oauth_callback))
        .route("/member/oauth/{provider}/unlink", post(web::oauth::oauth_unlink))
        // Board
        .route("/board/{bo_table}", get(web::board::list))
        .route("/board/{bo_table}/write", get(web::board::write_page).post(web::board::write_submit))
        .route("/board/{bo_table}/bulk-delete", post(web::board::bulk_delete_submit))
        .route("/board/{bo_table}/{post_id}", get(web::board::view))
        .route("/board/{bo_table}/{post_id}/edit", get(web::board::edit_page).post(web::board::edit_submit))
        .route("/board/{bo_table}/{post_id}/comment", post(web::board::comment_submit))
        .route("/board/{bo_table}/{post_id}/comment/{comment_id}/edit", post(web::board::comment_edit_submit))
        .route("/board/{bo_table}/{post_id}/comment/{comment_id}/delete", post(web::board::comment_delete_submit))
        .route("/board/{bo_table}/{post_id}/qna-accept/{comment_id}", post(web::board::qna_accept_submit))
        .route("/board/{bo_table}/{post_id}/delete", post(web::board::delete_submit))
        // Admin Dashboard
        .route("/admin", get(web::admin::dashboard))
        .route("/admin/settings", get(web::admin::settings_page).post(web::admin::settings_update))
        .route("/admin/oauth", get(web::oauth::admin_oauth_page))
        .route("/admin/oauth/save", post(web::oauth::admin_oauth_save))
        .route("/admin/themes", get(web::admin::themes_page))
        .route("/admin/themes/activate", post(web::admin::themes_activate))
        .route("/admin/members", get(web::admin::members_page))
        .route("/admin/members/update", post(web::admin::members_update))
        .route("/admin/members/bulk-delete", post(web::admin::members_bulk_delete))
        .route("/admin/member-levels", get(web::admin::member_levels_page))
        .route("/admin/member-levels/update", post(web::admin::member_levels_update))
        .route("/admin/boards", get(web::admin::boards_page))
        .route("/admin/boards/create", post(web::admin::boards_create))
        .route("/admin/boards/update", post(web::admin::boards_update))
        .route("/admin/files", get(web::admin::files_page))
        .route("/admin/files/bulk-delete", post(web::admin::files_bulk_delete))
        .route("/admin/points", get(web::admin::points_page))
        .route("/admin/trash", get(web::admin::trash_page))
        .route("/admin/trash/post/restore", post(web::admin::trash_post_restore))
        .route("/admin/trash/post/purge", post(web::admin::trash_post_purge))
        .route("/admin/trash/post/bulk-restore", post(web::admin::trash_post_bulk_restore))
        .route("/admin/trash/post/bulk-purge", post(web::admin::trash_post_bulk_purge))
        .route("/admin/trash/comment/restore", post(web::admin::trash_comment_restore))
        .route("/admin/trash/comment/purge", post(web::admin::trash_comment_purge))
        .route("/admin/trash/comment/bulk-restore", post(web::admin::trash_comment_bulk_restore))
        .route("/admin/trash/comment/bulk-purge", post(web::admin::trash_comment_bulk_purge))
        .route("/admin/trash/empty", post(web::admin::trash_empty))
        .route("/admin/backup", get(web::admin::backup_page))
        .route("/admin/backup/download", get(web::admin::backup_download))
        .route("/admin/reports", get(web::admin::reports_page))
        .route("/admin/reports/update", post(web::admin::reports_update))
        .route("/admin/ai-settings", get(web::ai_builder::admin_ai_settings_page).post(web::ai_builder::admin_ai_settings_save))
        .route("/admin/ai-settings/models", post(web::ai_builder::admin_ai_settings_models))
        .route("/admin/ai-builder", get(web::ai_builder::admin_ai_builder_page))
        .route("/admin/ai-builder/plan", post(web::ai_builder::admin_ai_builder_plan))
        .route("/admin/ai-builder/apply", post(web::ai_builder::admin_ai_builder_apply))
        .route("/admin/ai-builder/rollback", post(web::ai_builder::admin_ai_builder_rollback))
        // API Endpoints (Asamiya-Editor, Secure Download, Votes)
        .route("/api/editor/upload", post(api::editor_upload::upload_image))
        .route("/api/editor/file-upload", post(api::editor_upload::upload_file))
        .route("/api/download/{file_uuid}", get(api::download::download_file))
        .route("/api/board/{bo_table}/{post_id}/vote", post(api::board_api::vote_post))
        .route("/api/reports", post(api::report_api::submit_report))
        // Notifications API & SSE Stream
        .route("/api/notifications", get(api::notification_api::list_notifications))
        .route("/api/notifications/stream", get(api::notification_api::stream_notifications))
        .route("/api/notifications/read-all", post(api::notification_api::mark_all_read))
        .route("/api/notifications/{id}/read", post(api::notification_api::mark_read))
        // Theme Assets & Static Files
        .route("/theme-assets/{theme_name}/{*asset_path}", get(web::serve_theme_asset))
        .nest_service("/static", ServeDir::new("static"))
        .route("/uploads/{*key}", get(api::download::serve_upload_asset))
        .layer(DefaultBodyLimit::max(max_body))
        .with_state(state);

    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    tracing::info!("🚀 CRust 서버 시작: http://{} (활성 테마: {})", addr, active_theme_name);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
