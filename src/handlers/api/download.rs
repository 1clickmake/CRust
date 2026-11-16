use crate::error::AppError;
use crate::middlewares::auth::AuthContext;
use crate::services::file_service;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use tokio_util::io::ReaderStream;

pub async fn download_file(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(file_uuid): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let record =
        file_service::authorize_and_record_download(&state, &file_uuid, auth.member.as_ref())
            .await?;

    let safe_filename = record
        .bf_source
        .replace(['"', '\r', '\n', '\\'], "_");
    let content_disp = format!("attachment; filename=\"{}\"", safe_filename);

    if let Some(local_path) = state.storage.local_path(&record.bf_path) {
        let file = tokio::fs::File::open(&local_path)
            .await
            .map_err(|_| AppError::NotFound("디스크에서 파일을 찾을 수 없습니다.".into()))?;
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        let mime = record
            .mime_type
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let mut resp = (StatusCode::OK, body).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&mime)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
        );
        if let Ok(hv) = HeaderValue::from_str(&content_disp) {
            resp.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
        }
        return Ok(resp);
    }

    let bytes = state.storage.get(&record.bf_path).await?;
    let mut resp = (StatusCode::OK, bytes).into_response();
    if let Ok(hv) = HeaderValue::from_str(&content_disp) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    Ok(resp)
}

/// Serve public editor media assets from active StorageBackend (Local OR S3/R2/MinIO).
/// Restricted strictly to `editor/` keys so protected board attachments still enforce download checks.
pub async fn serve_upload_asset(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let clean = key.trim_start_matches('/');
    if clean.contains("..") || !clean.starts_with("editor/") {
        return Err(AppError::Forbidden("접근할 수 없는 경로입니다.".into()));
    }

    let mime = match clean.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "webp" => "image/webp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };

    let bytes = if let Some(local_path) = state.storage.local_path(clean) {
        tokio::fs::read(&local_path)
            .await
            .map(bytes::Bytes::from)
            .map_err(|_| AppError::NotFound("이미지를 찾을 수 없습니다.".into()))?
    } else {
        state
            .storage
            .get(clean)
            .await
            .map_err(|_| AppError::NotFound("원격 스토리지에서 이미지를 찾을 수 없습니다.".into()))?
    };

    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000"),
    );
    Ok(resp)
}
