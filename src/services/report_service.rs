use crate::error::AppError;
use crate::i18n::{get_site_default_lang, tr_sys};
use crate::services::notification_service;
use crate::state::AppState;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReportView {
    pub id: i64,
    pub reporter_id: i64,
    pub reporter_mb_id: String,
    pub reporter_mb_name: String,
    pub target_type: String,
    pub target_id: i64,
    pub bo_table: Option<String>,
    pub reason: String,
    pub status: String,
    pub admin_note: Option<String>,
    pub created_at: String,
}

pub async fn create_report(
    state: &AppState,
    reporter_id: i64,
    target_type: &str,
    target_id: i64,
    bo_table: Option<&str>,
    reason: &str,
) -> Result<i64, AppError> {
    if target_type != "post" && target_type != "comment" && target_type != "member" {
        return Err(AppError::BadRequest("유효하지 않은 신고 대상 유형입니다.".into()));
    }
    let clean_reason = reason.trim();
    if clean_reason.is_empty() {
        return Err(AppError::BadRequest("신고 사유를 입력해 주세요.".into()));
    }

    let dup: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM reports
         WHERE reporter_id = ? AND target_type = ? AND target_id = ? AND status = 'pending'",
    )
    .bind(reporter_id)
    .bind(target_type)
    .bind(target_id)
    .fetch_one(&state.db)
    .await?;

    if dup.0 > 0 {
        return Err(AppError::ApiError(
            StatusCode::CONFLICT,
            "이미 검토 대기 중인 동일 신고가 있습니다.".into(),
        ));
    }

    let res = sqlx::query(
        "INSERT INTO reports (reporter_id, target_type, target_id, bo_table, reason)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(reporter_id)
    .bind(target_type)
    .bind(target_id)
    .bind(bo_table)
    .bind(clean_reason)
    .execute(&state.db)
    .await?;

    let report_id = res.last_insert_rowid();

    // Notify all level 10 admins
    let admins: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM members WHERE mb_level >= 10 AND deleted_at IS NULL")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let sys_lang = get_site_default_lang(&state.db).await;
    let noti_msg = tr_sys(
        &sys_lang,
        "sys_msg.new_report_noti",
        &[("target_type", target_type), ("target_id", &target_id.to_string())],
    );
    for (aid,) in admins {
        let _ = notification_service::dispatch_notification(
            &state.db,
            &state.events,
            aid,
            "report",
            &noti_msg,
            Some("/admin/reports"),
        )
        .await;
    }

    Ok(report_id)
}

pub async fn list_reports(pool: &SqlitePool) -> Result<Vec<ReportView>, AppError> {
    let rows = sqlx::query_as::<_, ReportView>(
        "SELECT r.id, r.reporter_id, m.mb_id AS reporter_mb_id, m.mb_name AS reporter_mb_name,
                r.target_type, r.target_id, r.bo_table, r.reason, r.status, r.admin_note, r.created_at
         FROM reports r
         INNER JOIN members m ON m.id = r.reporter_id
         ORDER BY CASE WHEN r.status = 'pending' THEN 0 ELSE 1 END, r.id DESC
         LIMIT 200",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_report_status(
    pool: &SqlitePool,
    admin_id: i64,
    report_id: i64,
    status: &str,
    admin_note: Option<&str>,
) -> Result<(), AppError> {
    if status != "resolved" && status != "dismissed" && status != "pending" {
        return Err(AppError::BadRequest("유효하지 않은 상태 값입니다.".into()));
    }
    sqlx::query(
        "UPDATE reports
         SET status = ?, admin_note = ?, resolved_by = ?, resolved_at = CURRENT_TIMESTAMP
         WHERE id = ?",
    )
    .bind(status)
    .bind(admin_note.map(|s| s.trim()).filter(|s| !s.is_empty()))
    .bind(admin_id)
    .bind(report_id)
    .execute(pool)
    .await?;
    Ok(())
}
