use crate::error::AppError;
use crate::events::{DomainEvent, EventBus};
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn is_point_enabled(pool: &SqlitePool) -> bool {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT cf_value FROM site_configs WHERE cf_key = 'use_point'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    row.map(|(v,)| v != "0").unwrap_or(true)
}

pub async fn apply_point_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    member_id: i64,
    delta: i64,
    rel_table: Option<&str>,
    rel_id: Option<&str>,
    rel_action: &str,
    content: &str,
) -> Result<i64, AppError> {
    if delta == 0 {
        let (bal,): (i64,) = sqlx::query_as("SELECT mb_point FROM members WHERE id = ?")
            .bind(member_id)
            .fetch_one(&mut **tx)
            .await?;
        return Ok(bal);
    }

    let res = sqlx::query(
        "UPDATE members
         SET mb_point = mb_point + ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ? AND (mb_point + ?) >= 0",
    )
    .bind(delta)
    .bind(member_id)
    .bind(delta)
    .execute(&mut **tx)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::Forbidden("보유 포인트가 부족합니다.".into()));
    }

    let (balance_after,): (i64,) = sqlx::query_as("SELECT mb_point FROM members WHERE id = ?")
        .bind(member_id)
        .fetch_one(&mut **tx)
        .await?;

    sqlx::query(
        "INSERT INTO point_logs (member_id, rel_table, rel_id, rel_action, point, balance_after, content)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(member_id)
    .bind(rel_table)
    .bind(rel_id)
    .bind(rel_action)
    .bind(delta)
    .bind(balance_after)
    .bind(content)
    .execute(&mut **tx)
    .await?;

    Ok(balance_after)
}

pub async fn apply_point(
    pool: &SqlitePool,
    events: &EventBus,
    member_id: i64,
    delta: i64,
    rel_table: Option<&str>,
    rel_id: Option<&str>,
    rel_action: &str,
    content: &str,
) -> Result<i64, AppError> {
    let mut tx = pool.begin().await?;
    let bal = apply_point_in_tx(
        &mut tx,
        member_id,
        delta,
        rel_table,
        rel_id,
        rel_action,
        content,
    )
    .await?;
    tx.commit().await?;

    if delta != 0 {
        events.publish(DomainEvent::PointChanged {
            member_id,
            delta,
            balance_after: bal,
            action: rel_action.to_string(),
        });
    }

    Ok(bal)
}

pub async fn has_point_log(
    pool: &SqlitePool,
    member_id: i64,
    rel_table: &str,
    rel_id: &str,
    rel_action: &str,
) -> Result<bool, AppError> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM point_logs
         WHERE member_id = ? AND rel_table = ? AND rel_id = ? AND rel_action = ?",
    )
    .bind(member_id)
    .bind(rel_table)
    .bind(rel_id)
    .bind(rel_action)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
}
