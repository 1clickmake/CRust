use crate::error::AppError;
use crate::events::{DomainEvent, EventBus};
use crate::models::notification::Notification;
use sqlx::SqlitePool;

pub async fn create_notification(
    pool: &SqlitePool,
    member_id: i64,
    noti_type: &str,
    message: &str,
    target_url: Option<&str>,
) -> Result<i64, AppError> {
    let res = sqlx::query(
        "INSERT INTO notifications (member_id, noti_type, message, target_url)
         VALUES (?, ?, ?, ?)",
    )
    .bind(member_id)
    .bind(noti_type)
    .bind(message)
    .bind(target_url)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

/// Insert notification into SQLite and broadcast NotificationCreated on EventBus for real-time SSE clients.
pub async fn dispatch_notification(
    pool: &SqlitePool,
    events: &EventBus,
    member_id: i64,
    noti_type: &str,
    message: &str,
    target_url: Option<&str>,
) -> Result<i64, AppError> {
    let noti_id = create_notification(pool, member_id, noti_type, message, target_url).await?;
    events.publish(DomainEvent::NotificationCreated {
        member_id,
        noti_id,
        noti_type: noti_type.to_string(),
        message: message.to_string(),
        target_url: target_url.map(|s| s.to_string()),
    });
    Ok(noti_id)
}

pub async fn unread_count(pool: &SqlitePool, member_id: i64) -> Result<i64, AppError> {
    let (cnt,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notifications WHERE member_id = ? AND is_read = 0",
    )
    .bind(member_id)
    .fetch_one(pool)
    .await?;
    Ok(cnt)
}

pub async fn list_for_member(
    pool: &SqlitePool,
    member_id: i64,
    limit: i64,
) -> Result<Vec<Notification>, AppError> {
    let rows = sqlx::query_as::<_, Notification>(
        "SELECT id, member_id, noti_type, message, target_url, is_read, created_at
         FROM notifications WHERE member_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(member_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_as_read(
    pool: &SqlitePool,
    member_id: i64,
    noti_id: i64,
) -> Result<bool, AppError> {
    let res = sqlx::query(
        "UPDATE notifications SET is_read = 1 WHERE id = ? AND member_id = ? AND is_read = 0",
    )
    .bind(noti_id)
    .bind(member_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn mark_all_as_read(pool: &SqlitePool, member_id: i64) -> Result<u64, AppError> {
    let res = sqlx::query("UPDATE notifications SET is_read = 1 WHERE member_id = ? AND is_read = 0")
        .bind(member_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
