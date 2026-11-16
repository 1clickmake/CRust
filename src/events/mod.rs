pub mod listener;

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum DomainEvent {
    MemberJoined { member_id: i64, mb_id: String },
    PostCreated { post_id: i64, bo_table: String, member_id: Option<i64> },
    PostUpdated { post_id: i64, bo_table: String },
    PostDeleted { post_id: i64, bo_table: String },
    CommentCreated { comment_id: i64, post_id: i64, member_id: Option<i64> },
    PointChanged { member_id: i64, delta: i64, balance_after: i64, action: String },
    ThemeChanged { old_theme: String, new_theme: String },
    NotificationCreated {
        member_id: i64,
        noti_id: i64,
        noti_type: String,
        message: String,
        target_url: Option<String>,
    },
}

pub struct EventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}
