use crate::events::DomainEvent;
use crate::state::AppState;

/// Central background EventBus listener.
/// Provides an extensible hook point for audit logging, webhooks, and future domain automation.
pub fn spawn_event_listener(state: AppState) {
    let mut rx = state.events.subscribe();
    tokio::spawn(async move {
        while let Ok(evt) = rx.recv().await {
            match evt {
                DomainEvent::MemberJoined { member_id, mb_id } => {
                    tracing::info!("[EventBus] MemberJoined id={} mb_id={}", member_id, mb_id);
                }
                DomainEvent::PostCreated { post_id, bo_table, member_id } => {
                    tracing::info!(
                        "[EventBus] PostCreated board={} post_id={} member_id={:?}",
                        bo_table,
                        post_id,
                        member_id
                    );
                }
                DomainEvent::PostUpdated { post_id, bo_table } => {
                    tracing::info!("[EventBus] PostUpdated board={} post_id={}", bo_table, post_id);
                }
                DomainEvent::PostDeleted { post_id, bo_table } => {
                    tracing::info!("[EventBus] PostDeleted board={} post_id={}", bo_table, post_id);
                }
                DomainEvent::CommentCreated { comment_id, post_id, member_id } => {
                    tracing::info!(
                        "[EventBus] CommentCreated post_id={} comment_id={} member_id={:?}",
                        post_id,
                        comment_id,
                        member_id
                    );
                }
                DomainEvent::PointChanged { member_id, delta, balance_after, action } => {
                    tracing::info!(
                        "[EventBus] PointChanged member_id={} action={} delta={} balance={}",
                        member_id,
                        action,
                        delta,
                        balance_after
                    );
                }
                DomainEvent::ThemeChanged { old_theme, new_theme } => {
                    tracing::info!("[EventBus] ThemeChanged {} -> {}", old_theme, new_theme);
                }
                DomainEvent::NotificationCreated {
                    member_id,
                    noti_id,
                    noti_type,
                    message,
                    target_url,
                } => {
                    tracing::debug!(
                        "[EventBus] NotificationCreated id={} member_id={} type={} url={:?} msg={}",
                        noti_id,
                        member_id,
                        noti_type,
                        target_url,
                        message
                    );
                }
            }
        }
    });
}
