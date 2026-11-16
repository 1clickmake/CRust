use crate::error::AppError;
use crate::middlewares::auth::{extract_header_csrf, AuthContext};
use crate::services::report_service;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct CreateReportRequest {
    pub target_type: String,
    pub target_id: i64,
    pub bo_table: Option<String>,
    pub reason: String,
}

pub async fn submit_report(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    Json(req): Json<CreateReportRequest>,
) -> Result<Json<Value>, AppError> {
    if auth.verify_csrf(extract_header_csrf(&headers)).is_err() {
        return Err(AppError::ApiError(
            StatusCode::FORBIDDEN,
            "유효하지 않은 보안 토큰(CSRF)입니다.".into(),
        ));
    }

    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인 후 신고할 수 있습니다.".into())
    })?;

    let report_id = report_service::create_report(
        &state,
        member.id,
        &req.target_type,
        req.target_id,
        req.bo_table.as_deref(),
        &req.reason,
    )
    .await?;

    Ok(Json(json!({ "ok": true, "report_id": report_id })))
}
