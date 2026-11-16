use crate::error::AppError;
use crate::handlers::web::build_base_context;
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::services::{auth_service, oauth};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;

const OAUTH_STATE_COOKIE: &str = "rb_oauth_state";

#[derive(Deserialize)]
pub struct AdminOAuthSaveForm {
    pub provider: String,
    pub is_enabled: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub extra_config: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

fn infer_origin(headers: &HeaderMap, state: &AppState) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}:{}", state.config.server.host, state.config.server.port));
    format!("{}://{}", proto, host)
}

pub async fn admin_oauth_page(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let _admin = auth.require_admin()?;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (_theme, base) = build_base_context(&state, &auth, Some(csrf), None).await?;
    let providers = oauth::list_all_providers(&state.db).await?;
    let origin = infer_origin(&headers, &state);

    let html = state.render_template(
        "admin/oauth.html",
        context! {
            providers => providers,
            site_origin => origin,
            ..base
        },
    )?;
    Ok((jar, Html(html)))
}

pub async fn admin_oauth_save(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<AdminOAuthSaveForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    oauth::save_provider_config(
        &state.db,
        &form.provider,
        form.is_enabled.is_some(),
        &form.client_id,
        &form.client_secret,
        &form.scopes,
        &form.extra_config,
    )
    .await?;

    Ok(Redirect::to("/admin/oauth?saved=1"))
}

pub async fn oauth_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(provider): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let adapter = oauth::resolve_provider(&provider)
        .ok_or_else(|| AppError::NotFound("지원하지 않는 소셜 로그인 제공자입니다.".into()))?;
    let cfg = oauth::get_provider_config(&state.db, &provider).await?;
    if cfg.is_enabled == 0 {
        return Err(AppError::Forbidden(
            "관리자가 아직 활성화하지 않은 소셜 로그인입니다.".into(),
        ));
    }

    let origin = infer_origin(&headers, &state);
    let redirect_uri = format!("{}/auth/oauth/{}/callback", origin, provider);
    let state_nonce = auth_service::generate_random_token();
    let cookie_val = format!("{}:{}", provider, state_nonce);

    let auth_url = adapter.authorize_url(&cfg, &redirect_uri, &state_nonce)?;

    let cookie = Cookie::build((OAUTH_STATE_COOKIE, cookie_val))
        .path("/")
        .http_only(true)
        .secure(state.config.security.cookie_secure)
        .same_site(SameSite::Lax)
        .build();

    Ok((jar.add(cookie), Redirect::to(&auth_url)))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    jar: CookieJar,
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(err) = query.error {
        let msg = query.error_description.unwrap_or(err);
        return Err(AppError::Unauthorized(format!("소셜 인증 취소 또는 오류: {}", msg)));
    }

    let code = query
        .code
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("인증 코드(code)가 전달되지 않았습니다.".into()))?;
    let state_param = query
        .state
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("보안 상태값(state)이 누락되었습니다.".into()))?;

    let stored = jar
        .get(OAUTH_STATE_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| AppError::Forbidden("OAuth 세션 상태 쿠키가 만료되었습니다.".into()))?;

    let expected = format!("{}:{}", provider, state_param);
    if stored != expected {
        return Err(AppError::Forbidden("유효하지 않은 OAuth State 값입니다.".into()));
    }

    let adapter = oauth::resolve_provider(&provider)
        .ok_or_else(|| AppError::NotFound("지원하지 않는 소셜 로그인 제공자입니다.".into()))?;
    let cfg = oauth::get_provider_config(&state.db, &provider).await?;
    if cfg.is_enabled == 0 {
        return Err(AppError::Forbidden("비활성화된 소셜 로그인입니다.".into()));
    }

    let origin = infer_origin(&headers, &state);
    let redirect_uri = format!("{}/auth/oauth/{}/callback", origin, provider);
    let client = reqwest::Client::new();
    let user_info = adapter
        .fetch_user(&client, &cfg, code, &redirect_uri, state_param)
        .await?;

    let mut jar = jar.remove(Cookie::from(OAUTH_STATE_COOKIE));

    let res = oauth::complete_oauth_login_or_link(
        &state,
        auth.member.as_ref(),
        &provider,
        &cfg.display_name,
        user_info,
        &auth.client_ip,
        &auth.user_agent,
    )
    .await?;

    if let Some((_member, session_id)) = res {
        let sess_cookie = Cookie::build((
            state.config.security.session_cookie_name.clone(),
            session_id,
        ))
        .path("/")
        .http_only(true)
        .secure(state.config.security.cookie_secure)
        .same_site(SameSite::Lax)
        .build();
        jar = jar.add(sess_cookie);
        Ok((jar, Redirect::to("/")))
    } else {
        Ok((jar, Redirect::to("/member/profile")))
    }
}

#[derive(Deserialize)]
pub struct OAuthUnlinkForm {
    pub csrf_token: Option<String>,
}

pub async fn oauth_unlink(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(provider): Path<String>,
    Form(form): Form<OAuthUnlinkForm>,
) -> Result<Redirect, AppError> {
    let member = auth.require_member()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;
    oauth::unlink_member_provider(&state.db, member.id, &provider).await?;
    Ok(Redirect::to("/member/profile"))
}
