use crate::error::AppError;
use crate::i18n::LANG_COOKIE_NAME;
use crate::models::member::Member;
use crate::services::{auth_service, settings_service};
use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum_extra::extract::CookieJar;

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub member: Option<Member>,
    pub session_id: Option<String>,
    pub csrf_token: String,
    pub client_ip: String,
    pub user_agent: String,
    pub lang_cookie: Option<String>,
    pub accept_lang: Option<String>,
    pub base_url: String,
}

impl AuthContext {
    pub fn level(&self) -> i64 {
        self.member.as_ref().map(|m| m.mb_level).unwrap_or(1)
    }

    pub fn is_admin(&self) -> bool {
        self.level() >= 10
    }

    pub fn require_member(&self) -> Result<&Member, AppError> {
        self.member
            .as_ref()
            .ok_or_else(|| AppError::Unauthorized("로그인이 필요한 기능입니다.".into()))
    }

    pub fn require_admin(&self) -> Result<&Member, AppError> {
        let m = self.require_member()?;
        if m.mb_level < 10 {
            return Err(AppError::Forbidden("최고 관리자(Lv.10) 권한이 필요합니다.".into()));
        }
        Ok(m)
    }

    pub fn verify_csrf(&self, provided: Option<&str>) -> Result<(), AppError> {
        match provided {
            Some(tok) if !tok.is_empty() && tok == self.csrf_token => Ok(()),
            _ => Err(AppError::Forbidden("유효하지 않은 보안 토큰(CSRF)입니다. 페이지를 새로고침 후 다시 시도해주세요.".into())),
        }
    }
}

pub fn extract_header_csrf(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("X-CSRF-Token")
        .or_else(|| headers.get("x-csrf-token"))
        .and_then(|v| v.to_str().ok())
}

pub fn ensure_csrf_cookie(auth: &AuthContext, jar: CookieJar) -> (CookieJar, String) {
    if auth.member.is_some() || !auth.csrf_token.is_empty() {
        return (jar, auth.csrf_token.clone());
    }
    let token = auth_service::generate_random_token();
    let cookie = axum_extra::extract::cookie::Cookie::build(("rb_guest_csrf", token.clone()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .build();
    (jar.add(cookie), token)
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_cookie = jar
            .get(&state.config.security.session_cookie_name)
            .map(|c| c.value().to_string());

        let client_ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let user_agent = parts
            .headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown")
            .chars()
            .take(200)
            .collect();

        let lang_cookie = jar
            .get(LANG_COOKIE_NAME)
            .map(|c| c.value().to_string());
        let accept_lang = parts
            .headers
            .get("accept-language")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let base_url = crate::handlers::web::seo::resolve_base_url(&parts.headers, state);

        if let Some(sid) = session_cookie {
            if let Some((member, csrf_token)) =
                auth_service::get_session_member(&state.db, &sid).await
            {
                let is_admin_recovery =
                    member.mb_level >= 10 && parts.uri.path().starts_with("/admin");
                if !is_admin_recovery {
                    check_ip_access(&state.db, &client_ip).await?;
                }
                return Ok(Self {
                    member: Some(member),
                    session_id: Some(sid),
                    csrf_token,
                    client_ip,
                    user_agent,
                    lang_cookie,
                    accept_lang,
                    base_url,
                });
            }
        }

        check_ip_access(&state.db, &client_ip).await?;

        // Guest fallback CSRF cookie
        let guest_csrf = jar
            .get("rb_guest_csrf")
            .map(|c| c.value().to_string())
            .unwrap_or_default();

        Ok(Self {
            member: None,
            session_id: None,
            csrf_token: guest_csrf,
            client_ip,
            user_agent,
            lang_cookie,
            accept_lang,
            base_url,
        })
    }
}

async fn check_ip_access(pool: &sqlx::SqlitePool, client_ip: &str) -> Result<(), AppError> {
    let settings = settings_service::load_all(pool).await;
    if !settings.possible_ip.trim().is_empty()
        && !settings_service::ip_matches_list(&settings.possible_ip, client_ip)
    {
        return Err(AppError::Forbidden(
            "접근이 허용되지 않은 IP 주소입니다.".into(),
        ));
    }
    if !settings.intercept_ip.trim().is_empty()
        && settings_service::ip_matches_list(&settings.intercept_ip, client_ip)
    {
        return Err(AppError::Forbidden(
            "관리자에 의해 접근이 차단된 IP 주소입니다.".into(),
        ));
    }
    Ok(())
}
