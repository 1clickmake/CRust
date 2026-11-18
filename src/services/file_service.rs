use crate::error::AppError;
use crate::models::board::Board;
use crate::models::member::Member;
use crate::models::post::{Post, PostFile};
use crate::services::{image_processor, point_service, settings_service};
use crate::state::AppState;
use bytes::Bytes;
use chrono::Utc;
use regex::Regex;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::Path;
use uuid::Uuid;

const BLOCKED_EXTENSIONS: &[&str] = &[
    "exe", "bat", "cmd", "sh", "ps1", "msi", "dll", "php", "jsp", "asp", "aspx", "js", "html",
    "htm", "svg",
];

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

pub async fn upload_temp_file(
    state: &AppState,
    member_id: Option<i64>,
    original_name: &str,
    mime_type: Option<&str>,
    data: Bytes,
    is_image_only: bool,
) -> Result<PostFile, AppError> {
    let max_bytes = state.config.storage.max_file_size_mb * 1024 * 1024;
    if data.len() as u64 > max_bytes {
        return Err(AppError::ApiError(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "파일 크기가 최대 허용치({}MB)를 초과했습니다.",
                state.config.storage.max_file_size_mb
            ),
        ));
    }

    let ext = Path::new(original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if is_image_only {
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            return Err(AppError::ApiError(
                axum::http::StatusCode::BAD_REQUEST,
                "이미지 파일(jpg, jpeg, png, gif, webp)만 업로드 가능합니다.".into(),
            ));
        }
    } else if ext.is_empty() || BLOCKED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::ApiError(
            axum::http::StatusCode::BAD_REQUEST,
            "보안상 허용되지 않는 파일 확장자입니다.".into(),
        ));
    }

    let is_resizable_image = IMAGE_EXTENSIONS.contains(&ext.as_str());
    let (final_data, final_ext, final_mime) = if is_image_only || is_resizable_image {
        let cfg = settings_service::load_all(&state.db).await;
        let max_w = cfg.img_max_width;
        let to_webp = cfg.img_webp_enabled;
        let orig_ext_clone = ext.clone();
        let orig_mime_clone = mime_type.map(|s| s.to_string());
        let processed = tokio::task::spawn_blocking(move || {
            image_processor::process_uploaded_image(
                data,
                &orig_ext_clone,
                orig_mime_clone.as_deref(),
                max_w,
                to_webp,
            )
        })
        .await
        .map_err(|e| AppError::Internal(format!("Image task error: {}", e)))??;
        (processed.data, processed.ext, Some(processed.mime))
    } else {
        (data, ext, mime_type.map(|s| s.to_string()))
    };

    let file_uuid = Uuid::new_v4().to_string();
    let ym = Utc::now().format("%y%m").to_string();
    let stored_name = format!("{}.{}", file_uuid, final_ext);
    let file_type = if is_image_only { "image" } else { "attachment" };
    let folder = if is_image_only { "editor" } else { "board" };
    let rel_key = format!("{}/{}/{}", folder, ym, stored_name);

    state.storage.put(&rel_key, final_data.clone()).await?;

    let size = final_data.len() as i64;
    let res = sqlx::query(
        "INSERT INTO post_files
         (file_uuid, post_id, member_id, file_type, mime_type, storage_driver, bf_source, bf_file, bf_path, bf_filesize, is_temp, status)
         VALUES (?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, 1, 'active')",
    )
    .bind(&file_uuid)
    .bind(member_id)
    .bind(file_type)
    .bind(final_mime.as_deref())
    .bind(&state.config.storage.driver)
    .bind(original_name)
    .bind(&stored_name)
    .bind(&rel_key)
    .bind(size)
    .execute(&state.db)
    .await?;

    let id = res.last_insert_rowid();
    let record = sqlx::query_as::<_, PostFile>("SELECT * FROM post_files WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    Ok(record)
}

/// Extract UUIDs referenced in HTML (both file cards `/api/download/{uuid}` and editor images `/uploads/editor/YYMM/{uuid}.ext`)
pub fn extract_referenced_uuids(html: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let re = Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}").unwrap();
    for cap in re.find_iter(html) {
        set.insert(cap.as_str().to_lowercase());
    }
    set
}

/// Bind uploaded files to post on create/edit and delete files removed from content
pub async fn sync_post_files(
    state: &AppState,
    post_id: i64,
    member_id: Option<i64>,
    html_content: &str,
) -> Result<(), AppError> {
    let referenced = extract_referenced_uuids(html_content);

    // 1. Bind any matching temp files uploaded by this user (or same session)
    for uuid in &referenced {
        sqlx::query(
            "UPDATE post_files
             SET post_id = ?, is_temp = 0
             WHERE file_uuid = ? AND status = 'active' AND (post_id IS NULL OR post_id = ?)
               AND (member_id IS NULL OR member_id = ?)",
        )
        .bind(post_id)
        .bind(uuid)
        .bind(post_id)
        .bind(member_id)
        .execute(&state.db)
        .await?;
    }

    // 2. Find previously bound files on this post that were deleted from HTML during edit
    let existing_files = sqlx::query_as::<_, PostFile>(
        "SELECT * FROM post_files WHERE post_id = ? AND status = 'active'",
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await?;

    for f in existing_files {
        if !referenced.contains(&f.file_uuid.to_lowercase()) {
            sqlx::query("UPDATE post_files SET status = 'deleted' WHERE id = ?")
                .bind(f.id)
                .execute(&state.db)
                .await?;
            let _ = state.storage.delete(&f.bf_path).await;
        }
    }

    // 3. Update cached attachment count on posts
    let (att_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM post_files WHERE post_id = ? AND file_type = 'attachment' AND status = 'active'",
    )
    .bind(post_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query("UPDATE posts SET wr_file_count = ? WHERE id = ?")
        .bind(att_count)
        .bind(post_id)
        .execute(&state.db)
        .await?;

    Ok(())
}

/// Garbage-collect temp files older than 24 hours
pub async fn cleanup_orphan_temp_files(state: &AppState) -> Result<usize, AppError> {
    let orphans = sqlx::query_as::<_, PostFile>(
        "SELECT * FROM post_files
         WHERE is_temp = 1 AND status = 'active'
           AND created_at < DATETIME('now', '-24 hours')",
    )
    .fetch_all(&state.db)
    .await?;

    let mut count = 0;
    for f in orphans {
        let _ = state.storage.delete(&f.bf_path).await;
        sqlx::query("UPDATE post_files SET status = 'deleted' WHERE id = ?")
            .bind(f.id)
            .execute(&state.db)
            .await?;
        count += 1;
    }
    Ok(count)
}

pub async fn authorize_and_record_download(
    state: &AppState,
    file_uuid: &str,
    member: Option<&Member>,
) -> Result<PostFile, AppError> {
    let file = sqlx::query_as::<_, PostFile>(
        "SELECT * FROM post_files WHERE file_uuid = ? AND status = 'active'",
    )
    .bind(file_uuid)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("존재하지 않거나 삭제된 파일입니다.".into()))?;

    let user_level = member.map(|m| m.mb_level).unwrap_or(1);

    if let Some(post_id) = file.post_id {
        let post = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(post_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("연결된 게시글을 찾을 수 없습니다.".into()))?;

        let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE bo_table = ?")
            .bind(&post.bo_table)
            .fetch_one(&state.db)
            .await?;

        if user_level < board.bo_download_level {
            return Err(AppError::Forbidden(format!(
                "다운로드 권한이 없습니다. (필요 레벨: {})",
                board.bo_download_level
            )));
        }

        // Deduct download point if global point system is enabled and not already paid / not own post / not admin
        if board.bo_download_point != 0 && point_service::is_point_enabled(&state.db).await {
            if let Some(m) = member {
                let is_owner = post.member_id == Some(m.id);
                if !is_owner && m.mb_level < 10 {
                    let already_paid = point_service::has_point_log(
                        &state.db,
                        m.id,
                        "file",
                        &file.file_uuid,
                        "download",
                    )
                    .await?;

                    if !already_paid {
                        let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
                        let msg = crate::i18n::tr_sys(
                            &sys_lang,
                            "sys_msg.download_point",
                            &[("board", &board.bo_subject), ("file", &file.bf_source)],
                        );
                        point_service::apply_point(
                            &state.db,
                            &state.events,
                            m.id,
                            board.bo_download_point,
                            Some("file"),
                            Some(&file.file_uuid),
                            "download",
                            &msg,
                        )
                        .await?;
                    }
                }
            } else if board.bo_download_point < 0 {
                return Err(AppError::Unauthorized(
                    "포인트 차감 파일은 로그인 후 다운로드할 수 있습니다.".into(),
                ));
            }
        }
    }

    sqlx::query("UPDATE post_files SET bf_download_count = bf_download_count + 1 WHERE id = ?")
        .bind(file.id)
        .execute(&state.db)
        .await?;

    Ok(file)
}

pub async fn list_all_files(pool: &SqlitePool, limit: i64) -> Result<Vec<PostFile>, AppError> {
    let rows = sqlx::query_as::<_, PostFile>(
        "SELECT * FROM post_files WHERE status = 'active' ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
