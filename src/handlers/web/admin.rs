use crate::error::AppError;
use crate::handlers::web::build_base_context;
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::models::board::Board;
use crate::models::member::Member;
use crate::models::point::PointLog;
use crate::models::post::{Comment, Post, PostFile};
use crate::services::{auth_service, file_service, level_service, point_service, report_service, settings_service, theme_service};
use crate::state::{is_safe_slug, AppState};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ThemeActivateForm {
    pub theme_name: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct MemberUpdateForm {
    pub member_id: i64,
    pub mb_level: i64,
    pub point_delta: i64,
    pub point_reason: Option<String>,
    pub is_blocked: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct LevelConfigUpdateForm {
    pub level: i64,
    pub level_name: String,
    pub badge_icon: String,
    pub req_point: i64,
    pub req_posts: i64,
    pub req_comments: i64,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct BoardCreateForm {
    pub bo_table: String,
    pub bo_subject: String,
    pub bo_category_list: Option<String>,
    pub bo_skin: String,
    pub bo_read_level: i64,
    pub bo_write_level: i64,
    pub bo_write_point: i64,
    pub bo_download_point: i64,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct BoardUpdateForm {
    pub bo_table: String,
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
    pub csrf_token: Option<String>,
}

pub async fn dashboard(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;

    let (total_members,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM members WHERE deleted_at IS NULL")
            .fetch_one(&state.db)
            .await?;
    let (total_posts,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM posts WHERE deleted_at IS NULL")
            .fetch_one(&state.db)
            .await?;
    let (total_comments,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM comments WHERE deleted_at IS NULL")
            .fetch_one(&state.db)
            .await?;
    let (total_files,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM post_files WHERE status = 'active'")
            .fetch_one(&state.db)
            .await?;
    let (total_file_bytes,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(bf_filesize), 0) FROM post_files WHERE status = 'active'",
    )
    .fetch_one(&state.db)
    .await?;

    let active_theme = state.active_theme.read().await.clone();

    let html = state.render_template(
        "admin/index.html",
        context! {
            active_theme => active_theme,
            total_members => total_members,
            total_posts => total_posts,
            total_comments => total_comments,
            total_files => total_files,
            total_file_mb => format!("{:.2}", (total_file_bytes as f64) / 1048576.0),
            ..base
        },
    )?;

    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct TrashTargetForm {
    pub id: i64,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct TrashEmptyForm {
    pub csrf_token: Option<String>,
}

async fn purge_post_completely(state: &AppState, post_id: i64) -> Result<(), AppError> {
    let files = sqlx::query_as::<_, PostFile>("SELECT * FROM post_files WHERE post_id = ?")
        .bind(post_id)
        .fetch_all(&state.db)
        .await?;

    for f in files {
        let _ = state.storage.delete(&f.bf_path).await;
    }

    sqlx::query("DELETE FROM post_files WHERE post_id = ?")
        .bind(post_id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM comments WHERE post_id = ?")
        .bind(post_id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM posts WHERE id = ?")
        .bind(post_id)
        .execute(&state.db)
        .await?;

    Ok(())
}

pub async fn trash_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;

    let deleted_posts = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let deleted_comments = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let html = state.render_template(
        "admin/trash.html",
        context! {
            deleted_posts => deleted_posts,
            deleted_comments => deleted_comments,
            ..base
        },
    )?;
    Ok(Html(html))
}

pub async fn trash_post_restore(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<TrashTargetForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    sqlx::query("UPDATE posts SET deleted_at = NULL WHERE id = ?")
        .bind(form.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_post_purge(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<TrashTargetForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    purge_post_completely(&state, form.id).await?;
    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_comment_restore(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<TrashTargetForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let row: Option<(i64,)> =
        sqlx::query_as("SELECT post_id FROM comments WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(form.id)
            .fetch_optional(&state.db)
            .await?;

    if let Some((post_id,)) = row {
        sqlx::query("UPDATE comments SET deleted_at = NULL WHERE id = ?")
            .bind(form.id)
            .execute(&state.db)
            .await?;
        sqlx::query(
            "UPDATE posts SET wr_comment_count = (SELECT COUNT(*) FROM comments WHERE post_id = ? AND deleted_at IS NULL) WHERE id = ?",
        )
        .bind(post_id)
        .bind(post_id)
        .execute(&state.db)
        .await?;
    }

    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_comment_purge(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<TrashTargetForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    sqlx::query("DELETE FROM comments WHERE id = ? AND deleted_at IS NOT NULL")
        .bind(form.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_empty(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<TrashEmptyForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let post_ids: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM posts WHERE deleted_at IS NOT NULL")
            .fetch_all(&state.db)
            .await?;
    for (pid,) in post_ids {
        purge_post_completely(&state, pid).await?;
    }

    sqlx::query("DELETE FROM comments WHERE deleted_at IS NOT NULL")
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/admin/trash"))
}

#[derive(Deserialize)]
pub struct BulkIdsForm {
    pub ids_csv: Option<String>,
    pub csrf_token: Option<String>,
}

pub fn parse_ids_csv(raw: Option<&str>) -> Vec<i64> {
    raw.unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .filter(|&id| id > 0)
        .collect()
}

pub async fn trash_post_bulk_restore(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        sqlx::query("UPDATE posts SET deleted_at = NULL WHERE id = ?")
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_post_bulk_purge(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        purge_post_completely(&state, id).await?;
    }
    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_comment_bulk_restore(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT post_id FROM comments WHERE id = ? AND deleted_at IS NOT NULL")
                .bind(id)
                .fetch_optional(&state.db)
                .await?;

        if let Some((post_id,)) = row {
            sqlx::query("UPDATE comments SET deleted_at = NULL WHERE id = ?")
                .bind(id)
                .execute(&state.db)
                .await?;
            sqlx::query(
                "UPDATE posts SET wr_comment_count = (SELECT COUNT(*) FROM comments WHERE post_id = ? AND deleted_at IS NULL) WHERE id = ?",
            )
            .bind(post_id)
            .bind(post_id)
            .execute(&state.db)
            .await?;
        }
    }
    Ok(Redirect::to("/admin/trash"))
}

pub async fn trash_comment_bulk_purge(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        sqlx::query("DELETE FROM comments WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    Ok(Redirect::to("/admin/trash"))
}

pub async fn files_bulk_delete(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        if let Some(f) = sqlx::query_as::<_, PostFile>("SELECT * FROM post_files WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
        {
            let _ = state.storage.delete(&f.bf_path).await;
            sqlx::query("DELETE FROM post_files WHERE id = ?")
                .bind(id)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(Redirect::to("/admin/files"))
}

pub async fn members_bulk_delete(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    for id in parse_ids_csv(form.ids_csv.as_deref()) {
        if id == admin.id {
            continue;
        }
        sqlx::query("UPDATE members SET deleted_at = datetime('now') WHERE id = ? AND mb_id != 'admin'")
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    Ok(Redirect::to("/admin/members"))
}

pub async fn backup_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;

    let (cnt_members,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM members")
        .fetch_one(&state.db)
        .await?;
    let (cnt_posts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM posts")
        .fetch_one(&state.db)
        .await?;
    let (cnt_comments,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM comments")
        .fetch_one(&state.db)
        .await?;
    let (cnt_files,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM post_files")
        .fetch_one(&state.db)
        .await?;

    let db_bytes = tokio::fs::metadata("rust_board.db")
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let wal_bytes = tokio::fs::metadata("rust_board.db-wal")
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let html = state.render_template(
        "admin/backup.html",
        context! {
            cnt_members => cnt_members,
            cnt_posts => cnt_posts,
            cnt_comments => cnt_comments,
            cnt_files => cnt_files,
            db_kb => format!("{:.1}", (db_bytes as f64) / 1024.0),
            wal_kb => format!("{:.1}", (wal_bytes as f64) / 1024.0),
            ..base
        },
    )?;
    Ok(Html(html))
}

pub async fn backup_download(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<impl IntoResponse, AppError> {
    let _admin = auth.require_admin()?;

    let tmp_name = format!("rust_board_snap_{}.db", uuid::Uuid::new_v4());
    let tmp_path = std::env::temp_dir().join(&tmp_name);
    let escaped = tmp_path.to_string_lossy().replace('\'', "''");

    let sql = format!("VACUUM INTO '{}'", escaped);
    sqlx::query(&sql).execute(&state.db).await?;

    let bytes = tokio::fs::read(&tmp_path).await?;
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let stamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let disp = format!("attachment; filename=\"rust_board_backup_{}.db\"", stamp);

    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-sqlite3"),
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }

    Ok(resp)
}

pub async fn themes_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;

    let active = state.active_theme.read().await.clone();
    let themes = theme_service::list_available_themes(&active);

    let html = state.render_template(
        "admin/themes.html",
        context! {
            themes => themes,
            ..base
        },
    )?;
    Ok(Html(html))
}


pub async fn themes_activate(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<ThemeActivateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    theme_service::activate_theme(&state, &form.theme_name).await?;
    Ok(Redirect::to("/admin/themes"))
}

pub async fn members_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;

    let members = sqlx::query_as::<_, Member>(
        "SELECT * FROM members WHERE deleted_at IS NULL ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let levels = level_service::list_level_configs(&state.db).await?;

    let html = state.render_template(
        "admin/members.html",
        context! {
            members => members,
            levels => levels,
            ..base
        },
    )?;
    Ok(Html(html))
}


pub async fn members_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<MemberUpdateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let clamped_level = form.mb_level.clamp(1, 10);
    let blocked = if form.is_blocked.is_some() { 1 } else { 0 };

    sqlx::query(
        "UPDATE members SET mb_level = ?, is_blocked = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(clamped_level)
    .bind(blocked)
    .bind(form.member_id)
    .execute(&state.db)
    .await?;

    if blocked == 1 {
        // Immediately revoke active sessions of blocked member
        sqlx::query("DELETE FROM sessions WHERE member_id = ?")
            .bind(form.member_id)
            .execute(&state.db)
            .await?;
    }

    if form.point_delta != 0 {
        let reason = form
            .point_reason
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("관리자 수동 포인트 조정");
        point_service::apply_point(
            &state.db,
            &state.events,
            form.member_id,
            form.point_delta,
            Some("admin"),
            None,
            "admin",
            reason,
        )
        .await?;
    }

    Ok(Redirect::to("/admin/members"))
}

pub async fn member_levels_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;
    let levels = level_service::list_level_configs(&state.db).await?;

    let html = state.render_template(
        "admin/member_levels.html",
        context! {
            levels => levels,
            ..base
        },
    )?;
    Ok(Html(html))
}


pub async fn member_levels_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<LevelConfigUpdateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    if (1..=10).contains(&form.level) {
        sqlx::query(
            "UPDATE member_level_configs
             SET level_name = ?, badge_icon = ?, req_point = ?, req_posts = ?, req_comments = ?
             WHERE level = ?",
        )
        .bind(form.level_name.trim())
        .bind(form.badge_icon.trim())
        .bind(form.req_point.max(0))
        .bind(form.req_posts.max(0))
        .bind(form.req_comments.max(0))
        .bind(form.level)
        .execute(&state.db)
        .await?;
    }

    Ok(Redirect::to("/admin/member-levels"))
}

pub async fn boards_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (theme, base) = build_base_context(&state, &auth, None, None).await?;
    let boards = sqlx::query_as::<_, Board>("SELECT * FROM boards ORDER BY bo_table ASC")
        .fetch_all(&state.db)
        .await?;
    let available_skins = theme_service::list_available_skins(&theme);

    let html = state.render_template(
        "admin/boards.html",
        context! {
            boards => boards,
            available_skins => available_skins,
            ..base
        },
    )?;
    Ok(Html(html))
}


pub async fn boards_create(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BoardCreateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let bo_table = form.bo_table.trim();
    if !is_safe_slug(bo_table) {
        return Err(AppError::BadRequest(
            "게시판 코드는 영문, 숫자, -, _만 사용할 수 있습니다.".into(),
        ));
    }
    let skin = if is_safe_slug(form.bo_skin.trim()) {
        form.bo_skin.trim()
    } else {
        "basic"
    };

    sqlx::query(
        "INSERT INTO boards
         (bo_table, gr_id, bo_subject, bo_category_list, bo_skin, bo_read_level, bo_write_level, bo_write_point, bo_download_point)
         VALUES (?, 'community', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bo_table)
    .bind(form.bo_subject.trim())
    .bind(form.bo_category_list.as_deref().unwrap_or(""))
    .bind(skin)
    .bind(form.bo_read_level.clamp(1, 10))
    .bind(form.bo_write_level.clamp(1, 10))
    .bind(form.bo_write_point)
    .bind(form.bo_download_point)
    .execute(&state.db)
    .await?;

    Ok(Redirect::to("/admin/boards"))
}

pub async fn boards_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<BoardUpdateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let bo_table = form.bo_table.trim();
    if !is_safe_slug(bo_table) {
        return Err(AppError::BadRequest("올바르지 않은 게시판 코드입니다.".into()));
    }
    let subject = form.bo_subject.trim();
    if subject.is_empty() {
        return Err(AppError::BadRequest("게시판 이름을 입력해주세요.".into()));
    }
    let skin = if is_safe_slug(form.bo_skin.trim()) {
        form.bo_skin.trim()
    } else {
        "basic"
    };

    let res = sqlx::query(
        "UPDATE boards
         SET bo_subject = ?, bo_category_list = ?, bo_skin = ?,
             bo_list_level = ?, bo_read_level = ?, bo_write_level = ?, bo_comment_level = ?, bo_download_level = ?,
             bo_read_point = ?, bo_write_point = ?, bo_comment_point = ?, bo_download_point = ?
         WHERE bo_table = ?",
    )
    .bind(subject)
    .bind(form.bo_category_list.as_deref().unwrap_or("").trim())
    .bind(skin)
    .bind(form.bo_list_level.clamp(1, 10))
    .bind(form.bo_read_level.clamp(1, 10))
    .bind(form.bo_write_level.clamp(1, 10))
    .bind(form.bo_comment_level.clamp(1, 10))
    .bind(form.bo_download_level.clamp(1, 10))
    .bind(form.bo_read_point)
    .bind(form.bo_write_point)
    .bind(form.bo_comment_point)
    .bind(form.bo_download_point)
    .bind(bo_table)
    .execute(&state.db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("수정할 게시판을 찾을 수 없습니다.".into()));
    }

    Ok(Redirect::to("/admin/boards"))
}

pub async fn files_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;
    let files = file_service::list_all_files(&state.db, 100).await?;

    let html = state.render_template(
        "admin/files.html",
        context! {
            files => files,
            ..base
        },
    )?;
    Ok(Html(html))
}


#[derive(Deserialize)]
pub struct SettingsUpdateForm {
    pub site_title: String,
    pub site_description: Option<String>,
    pub admin_email: String,
    pub default_lang: Option<String>,
    pub page_rows: i64,
    pub write_pages: i64,
    pub analytics_script: Option<String>,
    pub add_meta: Option<String>,

    pub use_point: Option<String>,
    pub login_point: i64,
    pub register_point: i64,
    pub memo_send_point: i64,

    pub delay_sec: i64,
    pub filter_words: Option<String>,
    pub link_target: String,

    pub allow_register: Option<String>,
    pub register_level: i64,
    pub prohibit_id: Option<String>,
    pub stipulation: Option<String>,
    pub privacy: Option<String>,

    pub possible_ip: Option<String>,
    pub intercept_ip: Option<String>,
    pub img_max_width: Option<i64>,
    pub img_webp_enabled: Option<String>,
    pub csrf_token: Option<String>,
}

pub async fn settings_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;
    let settings = settings_service::load_all(&state.db).await;

    let html = state.render_template(
        "admin/settings.html",
        context! {
            settings => settings,
            client_ip => auth.client_ip,
            ..base
        },
    )?;
    Ok(Html(html))
}


pub async fn settings_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<SettingsUpdateForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let site_title = form.site_title.trim();
    if site_title.is_empty() {
        return Err(AppError::BadRequest("홈페이지 제목을 입력해주세요.".into()));
    }

    let s = settings_service::SiteSettings {
        site_title: site_title.to_string(),
        site_description: form.site_description.unwrap_or_default().trim().to_string(),
        admin_email: form.admin_email.trim().to_string(),
        default_lang: form.default_lang.unwrap_or_else(|| "ko".to_string()),
        page_rows: form.page_rows.clamp(5, 100),
        write_pages: form.write_pages.clamp(3, 20),
        analytics_script: form.analytics_script.unwrap_or_default(),
        add_meta: form.add_meta.unwrap_or_default(),
        use_point: form.use_point.is_some(),
        login_point: form.login_point.max(0),
        register_point: form.register_point.max(0),
        memo_send_point: form.memo_send_point.max(0),
        delay_sec: form.delay_sec.max(0),
        filter_words: form.filter_words.unwrap_or_default(),
        link_target: if form.link_target == "_self" {
            "_self".to_string()
        } else {
            "_blank".to_string()
        },
        allow_register: form.allow_register.is_some(),
        register_level: form.register_level.clamp(1, 10),
        prohibit_id: form.prohibit_id.unwrap_or_default(),
        stipulation: form.stipulation.unwrap_or_default(),
        privacy: form.privacy.unwrap_or_default(),
        possible_ip: form.possible_ip.unwrap_or_default(),
        intercept_ip: form.intercept_ip.unwrap_or_default(),
        img_max_width: form.img_max_width.unwrap_or(1920).clamp(0, 4096),
        img_webp_enabled: form.img_webp_enabled.is_some(),
    };

    settings_service::save_all(&state.db, &s).await?;
    Ok(Redirect::to("/admin/settings"))
}

pub async fn points_page(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Html<String>, AppError> {
    let _admin = auth.require_admin()?;
    let (_theme, base) = build_base_context(&state, &auth, None, None).await?;
    let logs = sqlx::query_as::<_, PointLog>(
        "SELECT * FROM point_logs ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let use_point = point_service::is_point_enabled(&state.db).await;
    let login_point = auth_service::get_site_config_i64(&state.db, "login_point", 100).await;
    let register_point = auth_service::get_site_config_i64(&state.db, "register_point", 1000).await;

    let html = state.render_template(
        "admin/points.html",
        context! {
            logs => logs,
            use_point => use_point,
            login_point => login_point,
            register_point => register_point,
            ..base
        },
    )?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct ReportUpdateForm {
    pub report_id: i64,
    pub status: String,
    pub admin_note: Option<String>,
    pub csrf_token: Option<String>,
}

pub async fn reports_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let _admin = auth.require_admin()?;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (_theme, base) = build_base_context(&state, &auth, Some(csrf), None).await?;
    let reports = report_service::list_reports(&state.db).await?;
    let html = state.render_template(
        "admin/reports.html",
        context! {
            reports => reports,
            ..base
        },
    )?;
    Ok((jar, Html(html)))
}

pub async fn reports_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<ReportUpdateForm>,
) -> Result<Redirect, AppError> {
    let admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;
    report_service::update_report_status(
        &state.db,
        admin.id,
        form.report_id,
        &form.status,
        form.admin_note.as_deref(),
    )
    .await?;
    Ok(Redirect::to("/admin/reports"))
}


