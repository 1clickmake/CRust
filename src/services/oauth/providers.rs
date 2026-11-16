use super::{OAuthProvider, OAuthProviderConfig, OAuthUserInfo};
use crate::error::AppError;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn extra_str(cfg: &OAuthProviderConfig, key: &str) -> Option<String> {
    let val: Value = serde_json::from_str(&cfg.extra_config).ok()?;
    val.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn build_auth_url(
    base: &str,
    client_id: &str,
    redirect_uri: &str,
    state_token: &str,
    scopes: &str,
    extra_params: &[(&str, &str)],
) -> String {
    let mut params = vec![
        format!("response_type=code"),
        format!("client_id={}", url_encode(client_id)),
        format!("redirect_uri={}", url_encode(redirect_uri)),
        format!("state={}", url_encode(state_token)),
    ];
    if !scopes.trim().is_empty() {
        params.push(format!("scope={}", url_encode(scopes.trim())));
    }
    for (k, v) in extra_params {
        params.push(format!("{}={}", k, url_encode(v)));
    }
    let sep = if base.contains('?') { "&" } else { "?" };
    format!("{}{}{}", base, sep, params.join("&"))
}

async fn exchange_token_and_fetch_json(
    client: &Client,
    token_url: &str,
    userinfo_url: &str,
    form_params: HashMap<&str, &str>,
) -> Result<Value, AppError> {
    let token_resp = client
        .post(token_url)
        .header("Accept", "application/json")
        .header("User-Agent", "CRust-OAuth/1.0")
        .form(&form_params)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("OAuth token request failed: {}", e)))?;

    if !token_resp.status().is_success() {
        let st = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        return Err(AppError::Unauthorized(format!(
            "소셜 로그인 토큰 교환에 실패했습니다 ({}): {}",
            st, body
        )));
    }

    let token_json: Value = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Invalid token JSON: {}", e)))?;

    let access_token = token_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Unauthorized("OAuth access_token 응답이 비어 있습니다.".into()))?;

    let user_resp = client
        .get(userinfo_url)
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .header("User-Agent", "CRust-OAuth/1.0")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("OAuth userinfo request failed: {}", e)))?;

    if !user_resp.status().is_success() {
        let st = user_resp.status();
        return Err(AppError::Unauthorized(format!(
            "소셜 사용자 정보 조회에 실패했습니다 ({}).",
            st
        )));
    }

    user_resp
        .json::<Value>()
        .await
        .map_err(|e| AppError::Internal(format!("Invalid userinfo JSON: {}", e)))
}

struct StandardOAuthSpec {
    default_auth_url: &'static str,
    default_token_url: &'static str,
    default_userinfo_url: &'static str,
}

impl StandardOAuthSpec {
    fn build_authorize(
        &self,
        cfg: &OAuthProviderConfig,
        redirect_uri: &str,
        state_token: &str,
        extra_params: &[(&str, &str)],
    ) -> Result<String, AppError> {
        if cfg.client_id.trim().is_empty() {
            return Err(AppError::BadRequest(format!(
                "{} Client ID가 설정되지 않았습니다.",
                cfg.display_name
            )));
        }
        let base = extra_str(cfg, "auth_url").unwrap_or_else(|| self.default_auth_url.to_string());
        Ok(build_auth_url(
            &base,
            cfg.client_id.trim(),
            redirect_uri,
            state_token,
            &cfg.scopes,
            extra_params,
        ))
    }

    async fn fetch_json(
        &self,
        client: &Client,
        cfg: &OAuthProviderConfig,
        code: &str,
        redirect_uri: &str,
        state_token: Option<&str>,
    ) -> Result<Value, AppError> {
        let token_url =
            extra_str(cfg, "token_url").unwrap_or_else(|| self.default_token_url.to_string());
        let userinfo_url =
            extra_str(cfg, "userinfo_url").unwrap_or_else(|| self.default_userinfo_url.to_string());

        let mut form = HashMap::new();
        form.insert("grant_type", "authorization_code");
        form.insert("client_id", cfg.client_id.trim());
        form.insert("client_secret", cfg.client_secret.trim());
        form.insert("redirect_uri", redirect_uri);
        form.insert("code", code);
        if let Some(st) = state_token {
            form.insert("state", st);
        }

        exchange_token_and_fetch_json(client, &token_url, &userinfo_url, form).await
    }
}

pub struct KakaoProvider;
const KAKAO_SPEC: StandardOAuthSpec = StandardOAuthSpec {
    default_auth_url: "https://kauth.kakao.com/oauth/authorize",
    default_token_url: "https://kauth.kakao.com/oauth/token",
    default_userinfo_url: "https://kapi.kakao.com/v2/user/me",
};

#[async_trait]
impl OAuthProvider for KakaoProvider {
    fn authorize_url(&self, cfg: &OAuthProviderConfig, redirect_uri: &str, state_token: &str) -> Result<String, AppError> {
        KAKAO_SPEC.build_authorize(cfg, redirect_uri, state_token, &[])
    }
    async fn fetch_user(&self, client: &Client, cfg: &OAuthProviderConfig, code: &str, redirect_uri: &str, _state_token: &str) -> Result<OAuthUserInfo, AppError> {
        let json = KAKAO_SPEC.fetch_json(client, cfg, code, redirect_uri, None).await?;
        let id = json.get("id")
            .and_then(|v| v.as_i64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))
            .ok_or_else(|| AppError::Unauthorized("Kakao 사용자 ID를 확인할 수 없습니다.".into()))?;
        let account = json.get("kakao_account");
        let email = account.and_then(|a| a.get("email")).and_then(|v| v.as_str()).map(|s| s.to_string());
        let nickname = account
            .and_then(|a| a.get("profile"))
            .and_then(|p| p.get("nickname"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(OAuthUserInfo { provider_user_id: id, email, nickname })
    }
}

pub struct NaverProvider;
const NAVER_SPEC: StandardOAuthSpec = StandardOAuthSpec {
    default_auth_url: "https://nid.naver.com/oauth2.0/authorize",
    default_token_url: "https://nid.naver.com/oauth2.0/token",
    default_userinfo_url: "https://openapi.naver.com/v1/nid/me",
};

#[async_trait]
impl OAuthProvider for NaverProvider {
    fn authorize_url(&self, cfg: &OAuthProviderConfig, redirect_uri: &str, state_token: &str) -> Result<String, AppError> {
        NAVER_SPEC.build_authorize(cfg, redirect_uri, state_token, &[])
    }
    async fn fetch_user(&self, client: &Client, cfg: &OAuthProviderConfig, code: &str, redirect_uri: &str, state_token: &str) -> Result<OAuthUserInfo, AppError> {
        let json = NAVER_SPEC.fetch_json(client, cfg, code, redirect_uri, Some(state_token)).await?;
        let resp = json.get("response").unwrap_or(&json);
        let id = resp.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
            .ok_or_else(|| AppError::Unauthorized("Naver 사용자 ID를 확인할 수 없습니다.".into()))?;
        let email = resp.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
        let nickname = resp.get("nickname").or_else(|| resp.get("name"))
            .and_then(|v| v.as_str()).map(|s| s.to_string());
        Ok(OAuthUserInfo { provider_user_id: id, email, nickname })
    }
}

pub struct GoogleProvider;
const GOOGLE_SPEC: StandardOAuthSpec = StandardOAuthSpec {
    default_auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    default_token_url: "https://oauth2.googleapis.com/token",
    default_userinfo_url: "https://www.googleapis.com/oauth2/v3/userinfo",
};

#[async_trait]
impl OAuthProvider for GoogleProvider {
    fn authorize_url(&self, cfg: &OAuthProviderConfig, redirect_uri: &str, state_token: &str) -> Result<String, AppError> {
        GOOGLE_SPEC.build_authorize(cfg, redirect_uri, state_token, &[("access_type", "online")])
    }
    async fn fetch_user(&self, client: &Client, cfg: &OAuthProviderConfig, code: &str, redirect_uri: &str, _state_token: &str) -> Result<OAuthUserInfo, AppError> {
        let json = GOOGLE_SPEC.fetch_json(client, cfg, code, redirect_uri, None).await?;
        let id = json.get("sub").or_else(|| json.get("id"))
            .and_then(|v| v.as_str()).map(|s| s.to_string())
            .ok_or_else(|| AppError::Unauthorized("Google 사용자 ID를 확인할 수 없습니다.".into()))?;
        let email = json.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
        let nickname = json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        Ok(OAuthUserInfo { provider_user_id: id, email, nickname })
    }
}

pub struct GitHubProvider;
const GITHUB_SPEC: StandardOAuthSpec = StandardOAuthSpec {
    default_auth_url: "https://github.com/login/oauth/authorize",
    default_token_url: "https://github.com/login/oauth/access_token",
    default_userinfo_url: "https://api.github.com/user",
};

#[async_trait]
impl OAuthProvider for GitHubProvider {
    fn authorize_url(&self, cfg: &OAuthProviderConfig, redirect_uri: &str, state_token: &str) -> Result<String, AppError> {
        GITHUB_SPEC.build_authorize(cfg, redirect_uri, state_token, &[])
    }
    async fn fetch_user(&self, client: &Client, cfg: &OAuthProviderConfig, code: &str, redirect_uri: &str, _state_token: &str) -> Result<OAuthUserInfo, AppError> {
        let json = GITHUB_SPEC.fetch_json(client, cfg, code, redirect_uri, None).await?;
        let id = json.get("id")
            .and_then(|v| v.as_i64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))
            .ok_or_else(|| AppError::Unauthorized("GitHub 사용자 ID를 확인할 수 없습니다.".into()))?;
        let email = json.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
        let nickname = json.get("name").or_else(|| json.get("login"))
            .and_then(|v| v.as_str()).map(|s| s.to_string());
        Ok(OAuthUserInfo { provider_user_id: id, email, nickname })
    }
}

pub struct AppleProvider;

#[async_trait]
impl OAuthProvider for AppleProvider {
    fn authorize_url(&self, cfg: &OAuthProviderConfig, redirect_uri: &str, state_token: &str) -> Result<String, AppError> {
        if cfg.client_id.trim().is_empty() {
            return Err(AppError::BadRequest("Apple Services ID (Client ID)가 설정되지 않았습니다.".into()));
        }
        let base = extra_str(cfg, "auth_url")
            .unwrap_or_else(|| "https://appleid.apple.com/auth/authorize".to_string());
        Ok(build_auth_url(
            &base,
            cfg.client_id.trim(),
            redirect_uri,
            state_token,
            &cfg.scopes,
            &[("response_mode", "query")],
        ))
    }
    async fn fetch_user(&self, client: &Client, cfg: &OAuthProviderConfig, code: &str, redirect_uri: &str, _state_token: &str) -> Result<OAuthUserInfo, AppError> {
        // Supports custom token_url/userinfo_url proxy (or configured ES256 client_secret)
        if let (Some(token_url), Some(userinfo_url)) = (extra_str(cfg, "token_url"), extra_str(cfg, "userinfo_url")) {
            let mut form = HashMap::new();
            form.insert("grant_type", "authorization_code");
            form.insert("client_id", cfg.client_id.trim());
            form.insert("client_secret", cfg.client_secret.trim());
            form.insert("redirect_uri", redirect_uri);
            form.insert("code", code);
            let json = exchange_token_and_fetch_json(client, &token_url, &userinfo_url, form).await?;
            let id = json.get("sub").and_then(|v| v.as_str()).map(|s| s.to_string())
                .ok_or_else(|| AppError::Unauthorized("Apple sub ID를 확인할 수 없습니다.".into()))?;
            let email = json.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
            let nickname = json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            return Ok(OAuthUserInfo { provider_user_id: id, email, nickname });
        }
        Err(AppError::BadRequest(
            "Apple Sign-In 토큰 교환에는 extra_config의 team_id/key_id/.p8 서명 JWT(client_secret) 또는 토큰 프록시 URL 설정이 필요합니다.".into(),
        ))
    }
}
