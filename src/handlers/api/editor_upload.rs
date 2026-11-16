use crate::error::AppError;
use crate::middlewares::auth::{extract_header_csrf, AuthContext};
use crate::services::file_service;
use crate::state::AppState;
use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

pub async fn upload_image(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    handle_upload(state, auth, headers, multipart, true).await
}

pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    handle_upload(state, auth, headers, multipart, false).await
}

async fn handle_upload(
    state: AppState,
    auth: AuthContext,
    headers: HeaderMap,
    mut multipart: Multipart,
    image_only: bool,
) -> Result<Json<Value>, AppError> {
    if auth.verify_csrf(extract_header_csrf(&headers)).is_err() {
        return Err(AppError::ApiError(
            StatusCode::FORBIDDEN,
            "유효하지 않은 보안 토큰(CSRF)입니다.".into(),
        ));
    }

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ApiError(StatusCode::BAD_REQUEST, format!("Multipart error: {}", e)))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload.bin".to_string());
        let mime = field.content_type().map(|s| s.to_string());
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::ApiError(StatusCode::BAD_REQUEST, format!("Read error: {}", e)))?;

        let record = file_service::upload_temp_file(
            &state,
            auth.member.as_ref().map(|m| m.id),
            &filename,
            mime.as_deref(),
            data,
            image_only,
        )
        .await?;

        if image_only {
            return Ok(Json(json!({
                "url": format!("/uploads/{}", record.bf_path),
                "file_uuid": record.file_uuid
            })));
        } else {
            return Ok(Json(json!({
                "file_uuid": record.file_uuid,
                "name": record.bf_source,
                "size": record.bf_filesize,
                "download_url": format!("/api/download/{}", record.file_uuid)
            })));
        }
    }

    Err(AppError::ApiError(
        StatusCode::BAD_REQUEST,
        "업로드할 파일(file)이 전송되지 않았습니다.".into(),
    ))
}
