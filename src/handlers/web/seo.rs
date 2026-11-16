use crate::error::AppError;
use crate::services::seo_service;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;

fn resolve_base_url(headers: &HeaderMap, state: &AppState) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .filter(|s| *s == "http" || *s == "https")
        .unwrap_or("http");

    if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
        let clean_host = host.trim();
        if !clean_host.is_empty() && !clean_host.contains('/') && !clean_host.contains('\\') {
            return format!("{}://{}", proto, clean_host);
        }
    }
    format!("http://{}:{}", state.config.server.host, state.config.server.port)
}

pub async fn robots_txt(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let base = resolve_base_url(&headers, &state);
    let body = seo_service::build_robots_txt(&base);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

pub async fn sitemap_xml(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let base = resolve_base_url(&headers, &state);
    let xml = seo_service::build_sitemap_xml(&state.db, &base).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        xml,
    ))
}

pub async fn rss_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let base = resolve_base_url(&headers, &state);
    let xml = seo_service::build_rss_xml(&state.db, &base).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        xml,
    ))
}
