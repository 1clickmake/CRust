use crate::error::AppError;
use crate::handlers::web::{build_base_context, PreviewQuery};
use crate::i18n::{is_supported_lang, LANG_COOKIE_NAME};
use crate::middlewares::auth::AuthContext;
use crate::models::post::Post;
use crate::services::{seo_service, settings_service, theme_service};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use minijinja::context;

pub async fn set_language(
    Path(locale): Path<String>,
    headers: HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    let target_redirect = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.starts_with('/') || s.contains("://"))
        .unwrap_or("/");

    if is_supported_lang(&locale) {
        let cookie = Cookie::build((LANG_COOKIE_NAME, locale))
            .path("/")
            .permanent()
            .same_site(SameSite::Lax)
            .build();
        (jar.add(cookie), Redirect::to(target_redirect))
    } else {
        (jar, Redirect::to(target_redirect))
    }
}

pub async fn index(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<PreviewQuery>,
) -> Result<Html<String>, AppError> {
    let (theme, base) =
        build_base_context(&state, &auth, None, q.preview_theme.as_deref()).await?;

    let latest_notices = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE bo_table = 'notice' AND deleted_at IS NULL ORDER BY id DESC LIMIT 5",
    )
    .fetch_all(&state.db)
    .await?;

    let latest_free = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE bo_table = 'free' AND deleted_at IS NULL ORDER BY id DESC LIMIT 10",
    )
    .fetch_all(&state.db)
    .await?;

    let settings = settings_service::load_all(&state.db).await;
    let base_trimmed = auth.base_url.trim_end_matches('/');
    let canonical_url = format!("{}/", base_trimmed);
    let json_ld = seo_service::build_website_json_ld(
        base_trimmed,
        &settings.site_title,
        &settings.site_description,
    );

    let tpl = theme_service::resolve_template(&theme, "index.html");
    let html = state.render_template(
        &tpl,
        context! {
            latest_notices => latest_notices,
            latest_free => latest_free,
            canonical_url => canonical_url,
            json_ld => json_ld,
            ..base
        },
    )?;

    Ok(Html(html))
}
