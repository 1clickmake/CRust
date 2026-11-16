use crate::error::AppError;
use crate::middlewares::auth::{extract_header_csrf, AuthContext};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct VoteRequest {
    pub vote_type: String,
}

pub async fn vote_post(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    Path((_bo_table, post_id)): Path<(String, i64)>,
    Json(req): Json<VoteRequest>,
) -> Result<Json<Value>, AppError> {
    if auth.verify_csrf(extract_header_csrf(&headers)).is_err() {
        return Err(AppError::ApiError(
            StatusCode::FORBIDDEN,
            "유효하지 않은 보안 토큰(CSRF)입니다.".into(),
        ));
    }

    let member = auth.member.ok_or_else(|| {
        AppError::ApiError(StatusCode::UNAUTHORIZED, "로그인 후 추천할 수 있습니다.".into())
    })?;

    if req.vote_type != "good" && req.vote_type != "nogood" {
        return Err(AppError::ApiError(
            StatusCode::BAD_REQUEST,
            "잘못된 추천 유형입니다.".into(),
        ));
    }

    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO post_votes (post_id, member_id, vote_type) VALUES (?, ?, ?)",
    )
    .bind(post_id)
    .bind(member.id)
    .bind(&req.vote_type)
    .execute(&state.db)
    .await?;

    if inserted.rows_affected() == 0 {
        return Err(AppError::ApiError(
            StatusCode::CONFLICT,
            "이미 추천/비추천 하신 게시글입니다.".into(),
        ));
    }

    let col = if req.vote_type == "good" {
        "wr_good"
    } else {
        "wr_nogood"
    };
    let sql = format!("UPDATE posts SET {} = {} + 1 WHERE id = ?", col, col);
    sqlx::query(&sql).bind(post_id).execute(&state.db).await?;

    let (good, nogood): (i64, i64) =
        sqlx::query_as("SELECT wr_good, wr_nogood FROM posts WHERE id = ?")
            .bind(post_id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(json!({ "wr_good": good, "wr_nogood": nogood })))
}
