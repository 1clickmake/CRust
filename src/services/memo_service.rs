use crate::error::AppError;
use crate::i18n::{get_site_default_lang, tr_sys};
use crate::models::member::Member;
use crate::services::{notification_service, point_service, settings_service};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoView {
    pub id: i64,
    pub sender_id: i64,
    pub receiver_id: i64,
    pub sender_mb_id: String,
    pub sender_mb_name: String,
    pub receiver_mb_id: String,
    pub receiver_mb_name: String,
    pub me_subject: String,
    pub me_content: String,
    pub is_read: i64,
    pub read_at: Option<String>,
    pub created_at: String,
}

pub async fn send_memo(
    state: &AppState,
    sender: &Member,
    receiver_mb_id: &str,
    subject: &str,
    content: &str,
) -> Result<i64, AppError> {
    let target_key = receiver_mb_id.trim();
    let subj = subject.trim();
    let body = content.trim();

    if target_key.is_empty() || subj.is_empty() || body.is_empty() {
        return Err(AppError::BadRequest("받는이, 제목, 내용을 모두 입력해 주세요.".into()));
    }

    let receiver = sqlx::query_as::<_, Member>(
        "SELECT * FROM members WHERE mb_id = ? AND deleted_at IS NULL",
    )
    .bind(target_key)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("존재하지 않는 회원 아이디입니다.".into()))?;

    if receiver.id == sender.id {
        return Err(AppError::BadRequest("자기 자신에게는 쪽지를 보낼 수 없습니다.".into()));
    }

    let settings = settings_service::load_all(&state.db).await;
    if sender.mb_level < 10 && settings.use_point && settings.memo_send_point > 0 {
        let sys_lang = get_site_default_lang(&state.db).await;
        let pt_msg = tr_sys(&sys_lang, "sys_msg.memo_send_point", &[("to", &receiver.mb_id)]);
        point_service::apply_point(
            &state.db,
            &state.events,
            sender.id,
            -settings.memo_send_point,
            Some("memo"),
            Some(&receiver.mb_id),
            "memo_send",
            &pt_msg,
        )
        .await?;
    }

    let res = sqlx::query(
        "INSERT INTO memos (sender_id, receiver_id, me_subject, me_content)
         VALUES (?, ?, ?, ?)",
    )
    .bind(sender.id)
    .bind(receiver.id)
    .bind(subj)
    .bind(body)
    .execute(&state.db)
    .await?;

    let memo_id = res.last_insert_rowid();

    let sys_lang = get_site_default_lang(&state.db).await;
    let noti_msg = tr_sys(
        &sys_lang,
        "sys_msg.new_memo_noti",
        &[("sender", &sender.mb_name), ("subject", subj)],
    );
    let _ = notification_service::dispatch_notification(
        &state.db,
        &state.events,
        receiver.id,
        "memo",
        &noti_msg,
        Some(&format!("/member/memo?view={}", memo_id)),
    )
    .await;

    Ok(memo_id)
}

pub async fn list_inbox(pool: &SqlitePool, member_id: i64) -> Result<Vec<MemoView>, AppError> {
    let rows = sqlx::query_as::<_, MemoView>(
        "SELECT m.id, m.sender_id, m.receiver_id,
                s.mb_id AS sender_mb_id, s.mb_name AS sender_mb_name,
                r.mb_id AS receiver_mb_id, r.mb_name AS receiver_mb_name,
                m.me_subject, m.me_content, m.is_read, m.read_at, m.created_at
         FROM memos m
         INNER JOIN members s ON s.id = m.sender_id
         INNER JOIN members r ON r.id = m.receiver_id
         WHERE m.receiver_id = ? AND m.receiver_deleted = 0
         ORDER BY m.id DESC LIMIT 100",
    )
    .bind(member_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_sent(pool: &SqlitePool, member_id: i64) -> Result<Vec<MemoView>, AppError> {
    let rows = sqlx::query_as::<_, MemoView>(
        "SELECT m.id, m.sender_id, m.receiver_id,
                s.mb_id AS sender_mb_id, s.mb_name AS sender_mb_name,
                r.mb_id AS receiver_mb_id, r.mb_name AS receiver_mb_name,
                m.me_subject, m.me_content, m.is_read, m.read_at, m.created_at
         FROM memos m
         INNER JOIN members s ON s.id = m.sender_id
         INNER JOIN members r ON r.id = m.receiver_id
         WHERE m.sender_id = ? AND m.sender_deleted = 0
         ORDER BY m.id DESC LIMIT 100",
    )
    .bind(member_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_and_mark_read(
    pool: &SqlitePool,
    member_id: i64,
    memo_id: i64,
) -> Result<Option<MemoView>, AppError> {
    let memo = sqlx::query_as::<_, MemoView>(
        "SELECT m.id, m.sender_id, m.receiver_id,
                s.mb_id AS sender_mb_id, s.mb_name AS sender_mb_name,
                r.mb_id AS receiver_mb_id, r.mb_name AS receiver_mb_name,
                m.me_subject, m.me_content, m.is_read, m.read_at, m.created_at
         FROM memos m
         INNER JOIN members s ON s.id = m.sender_id
         INNER JOIN members r ON r.id = m.receiver_id
         WHERE m.id = ? AND (
            (m.receiver_id = ? AND m.receiver_deleted = 0)
            OR (m.sender_id = ? AND m.sender_deleted = 0)
         )",
    )
    .bind(memo_id)
    .bind(member_id)
    .bind(member_id)
    .fetch_optional(pool)
    .await?;

    if let Some(mut m) = memo {
        if m.receiver_id == member_id && m.is_read == 0 {
            sqlx::query("UPDATE memos SET is_read = 1, read_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(m.id)
                .execute(pool)
                .await?;
            m.is_read = 1;
        }
        Ok(Some(m))
    } else {
        Ok(None)
    }
}

pub async fn delete_memo(pool: &SqlitePool, member_id: i64, memo_id: i64) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE memos
         SET receiver_deleted = CASE WHEN receiver_id = ? THEN 1 ELSE receiver_deleted END,
             sender_deleted = CASE WHEN sender_id = ? THEN 1 ELSE sender_deleted END
         WHERE id = ? AND (receiver_id = ? OR sender_id = ?)",
    )
    .bind(member_id)
    .bind(member_id)
    .bind(memo_id)
    .bind(member_id)
    .bind(member_id)
    .execute(pool)
    .await?;
    Ok(())
}
