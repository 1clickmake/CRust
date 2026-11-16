use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Internal(String),
    ApiError(StatusCode, String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::BadRequest(m) => write!(f, "BadRequest: {}", m),
            AppError::Unauthorized(m) => write!(f, "Unauthorized: {}", m),
            AppError::Forbidden(m) => write!(f, "Forbidden: {}", m),
            AppError::NotFound(m) => write!(f, "NotFound: {}", m),
            AppError::Internal(m) => write!(f, "Internal: {}", m),
            AppError::ApiError(c, m) => write!(f, "ApiError({}): {}", c, m),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::ApiError(status, msg) => {
                (status, Json(json!({ "error": msg }))).into_response()
            }
            AppError::BadRequest(msg) => render_error_page(StatusCode::BAD_REQUEST, "잘못된 요청", &msg),
            AppError::Unauthorized(msg) => render_error_page(StatusCode::UNAUTHORIZED, "로그인이 필요합니다", &msg),
            AppError::Forbidden(msg) => render_error_page(StatusCode::FORBIDDEN, "접근 권한이 없습니다", &msg),
            AppError::NotFound(msg) => render_error_page(StatusCode::NOT_FOUND, "페이지를 찾을 수 없습니다", &msg),
            AppError::Internal(msg) => {
                tracing::error!("Internal server error: {}", msg);
                render_error_page(StatusCode::INTERNAL_SERVER_ERROR, "서버 내부 오류", &msg)
            }
        }
    }
}

fn render_error_page(status: StatusCode, title: &str, msg: &str) -> Response {
    let escaped_title = ammonia::clean_text(title);
    let escaped_msg = ammonia::clean_text(msg);
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{} - CRust</title>
<style>
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background:#0f172a; color:#f8fafc; display:flex; align-items:center; justify-content:center; min-height:100vh; margin:0; }}
  .box {{ max-width:460px; width:90%; background:#1e293b; border:1px solid #334155; border-radius:16px; padding:32px; text-align:center; box-shadow:0 20px 50px rgba(0,0,0,0.4); }}
  .code {{ font-size:48px; font-weight:800; color:#60a5fa; margin-bottom:8px; }}
  h1 {{ font-size:20px; margin:0 0 12px; }}
  p {{ color:#cbd5e1; line-height:1.6; margin:0 0 24px; }}
  a {{ display:inline-block; padding:10px 20px; background:#3b82f6; color:#fff; text-decoration:none; border-radius:8px; font-weight:600; }}
  a:hover {{ background:#2563eb; }}
</style>
</head>
<body>
  <div class="box">
    <div class="code">{}</div>
    <h1>{}</h1>
    <p>{}</p>
    <a href="javascript:history.back()">이전으로</a>
    <a href="/" style="background:#475569; margin-left:8px;">홈으로</a>
  </div>
</body>
</html>"#,
        escaped_title,
        status.as_u16(),
        escaped_title,
        escaped_msg
    );
    (status, Html(html)).into_response()
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Internal(format!("Database error: {}", err))
    }
}

impl From<minijinja::Error> for AppError {
    fn from(err: minijinja::Error) -> Self {
        AppError::Internal(format!("Template render error: {}", err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(format!("IO error: {}", err))
    }
}
