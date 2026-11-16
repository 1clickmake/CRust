use crate::error::AppError;
use crate::events::EventBus;
use crate::models::member::MemberLevelConfig;
use crate::services::notification_service;
use sqlx::SqlitePool;

pub async fn list_level_configs(pool: &SqlitePool) -> Result<Vec<MemberLevelConfig>, AppError> {
    let rows = sqlx::query_as::<_, MemberLevelConfig>(
        "SELECT level, level_name, badge_icon, req_point, req_posts, req_comments
         FROM member_level_configs ORDER BY level ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn evaluate_auto_level_up(
    pool: &SqlitePool,
    events: &EventBus,
    member_id: i64,
) -> Result<(), AppError> {
    let member_row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT mb_level, mb_point FROM members WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(member_id)
    .fetch_optional(pool)
    .await?;

    let Some((current_level, mb_point)) = member_row else {
        return Ok(());
    };

    // Do not touch admin or sub-admin levels (9, 10)
    if current_level >= 9 {
        return Ok(());
    }

    let (post_cnt,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE member_id = ? AND deleted_at IS NULL",
    )
    .bind(member_id)
    .fetch_one(pool)
    .await?;

    let (comment_cnt,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM comments WHERE member_id = ? AND deleted_at IS NULL",
    )
    .bind(member_id)
    .fetch_one(pool)
    .await?;

    let configs = list_level_configs(pool).await?;
    let mut best_level = current_level;
    let mut best_name = String::new();

    for cfg in configs {
        if cfg.level <= current_level || cfg.level >= 9 {
            continue;
        }
        // Only auto-promote into levels that actually define at least one requirement
        if cfg.req_point == 0 && cfg.req_posts == 0 && cfg.req_comments == 0 {
            continue;
        }
        if mb_point >= cfg.req_point && post_cnt >= cfg.req_posts && comment_cnt >= cfg.req_comments
        {
            best_level = cfg.level;
            best_name = cfg.level_name;
        }
    }

    if best_level > current_level {
        sqlx::query("UPDATE members SET mb_level = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(best_level)
            .bind(member_id)
            .execute(pool)
            .await?;

        let sys_lang = crate::i18n::get_site_default_lang(pool).await;
        let msg = crate::i18n::tr_sys(
            &sys_lang,
            "sys_msg.level_up_noti",
            &[("name", &best_name), ("level", &best_level.to_string())],
        );
        let _ = notification_service::dispatch_notification(
            pool,
            events,
            member_id,
            "level_up",
            &msg,
            Some("/member/profile"),
        )
        .await;
    }

    Ok(())
}
