use crate::error::AppError;
use crate::events::DomainEvent;
use crate::middlewares::auth::{extract_header_csrf, AuthContext};
use crate::services::notification_service;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use serde_json::{json, Value};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<Value>, AppError> {
    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인이 필요합니다.".into())
    })?;

    let unread = notification_service::unread_count(&state.db, member.id).await?;
    let items = notification_service::list_for_member(&state.db, member.id, 15).await?;

    Ok(Json(json!({
        "unread_count": unread,
        "items": items
    })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    Path(noti_id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    if auth.verify_csrf(extract_header_csrf(&headers)).is_err() {
        return Err(AppError::ApiError(
            StatusCode::FORBIDDEN,
            "유효하지 않은 보안 토큰(CSRF)입니다.".into(),
        ));
    }

    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인이 필요합니다.".into())
    })?;

    let updated = notification_service::mark_as_read(&state.db, member.id, noti_id).await?;
    let unread = notification_service::unread_count(&state.db, member.id).await?;

    Ok(Json(json!({
        "ok": true,
        "updated": updated,
        "unread_count": unread
    })))
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    if auth.verify_csrf(extract_header_csrf(&headers)).is_err() {
        return Err(AppError::ApiError(
            StatusCode::FORBIDDEN,
            "유효하지 않은 보안 토큰(CSRF)입니다.".into(),
        ));
    }

    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인이 필요합니다.".into())
    })?;

    let updated_count = notification_service::mark_all_as_read(&state.db, member.id).await?;
    Ok(Json(json!({
        "ok": true,
        "updated_count": updated_count,
        "unread_count": 0
    })))
}

pub async fn stream_notifications(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인이 필요합니다.".into())
    })?;
    let uid = member.id;
    let rx = state.events.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |res| match res {
        Ok(DomainEvent::NotificationCreated {
            member_id,
            noti_id,
            noti_type,
            message,
            target_url,
        }) if member_id == uid => {
            let payload = json!({
                "id": noti_id,
                "noti_type": noti_type,
                "message": message,
                "target_url": target_url,
                "is_read": 0
            });
            let ev = Event::default()
                .event("notification")
                .json_data(payload)
                .unwrap_or_else(|_| Event::default().event("notification").data("update"));
            Some(Ok(ev))
        }
        _ => None,
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
