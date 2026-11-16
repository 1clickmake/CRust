pub mod providers;

use crate::error::AppError;
use crate::events::DomainEvent;
use crate::i18n::{get_site_default_lang, tr_sys};
use crate::models::member::Member;
use crate::services::{auth_service, notification_service, point_service, settings_service};
use crate::state::AppState;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OAuthProviderConfig {
    pub provider: String,
    pub display_name: String,
    pub is_enabled: i64,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub extra_config: String,
    pub sort_order: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    pub provider_user_id: String,
    pub email: Option<String>,
    pub nickname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemberOAuthLink {
    pub id: i64,
    pub member_id: i64,
    pub provider: String,
    pub provider_user_id: String,
    pub provider_email: Option<String>,
    pub provider_name: Option<String>,
    pub created_at: String,
}

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    fn authorize_url(
        &self,
        cfg: &OAuthProviderConfig,
        redirect_uri: &str,
        state_token: &str,
    ) -> Result<String, AppError>;
    async fn fetch_user(
        &self,
        client: &Client,
        cfg: &OAuthProviderConfig,
        code: &str,
        redirect_uri: &str,
        state_token: &str,
    ) -> Result<OAuthUserInfo, AppError>;
}

/// Extensible provider registry: add new adapters here without modifying core auth handlers.
pub fn resolve_provider(provider: &str) -> Option<Box<dyn OAuthProvider>> {
    match provider {
        "kakao" => Some(Box::new(providers::KakaoProvider)),
        "naver" => Some(Box::new(providers::NaverProvider)),
        "google" => Some(Box::new(providers::GoogleProvider)),
        "github" => Some(Box::new(providers::GitHubProvider)),
        "apple" => Some(Box::new(providers::AppleProvider)),
        _ => None,
    }
}

pub async fn list_all_providers(pool: &SqlitePool) -> Result<Vec<OAuthProviderConfig>, AppError> {
    let rows = sqlx::query_as::<_, OAuthProviderConfig>(
        "SELECT * FROM oauth_providers ORDER BY sort_order ASC, provider ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_enabled_providers(pool: &SqlitePool) -> Vec<OAuthProviderConfig> {
    sqlx::query_as::<_, OAuthProviderConfig>(
        "SELECT * FROM oauth_providers WHERE is_enabled = 1 AND client_id != '' ORDER BY sort_order ASC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

pub async fn list_member_links(pool: &SqlitePool, member_id: i64) -> Vec<MemberOAuthLink> {
    sqlx::query_as::<_, MemberOAuthLink>(
        "SELECT * FROM member_oauth_links WHERE member_id = ? ORDER BY id ASC",
    )
    .bind(member_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

pub async fn unlink_member_provider(
    pool: &SqlitePool,
    member_id: i64,
    provider: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM member_oauth_links WHERE member_id = ? AND provider = ?")
        .bind(member_id)
        .bind(provider)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_provider_config(
    pool: &SqlitePool,
    provider: &str,
) -> Result<OAuthProviderConfig, AppError> {
    sqlx::query_as::<_, OAuthProviderConfig>("SELECT * FROM oauth_providers WHERE provider = ?")
        .bind(provider)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("지원하지 않는 소셜 로그인 제공자입니다.".into()))
}

pub async fn save_provider_config(
    pool: &SqlitePool,
    provider: &str,
    is_enabled: bool,
    client_id: &str,
    client_secret: &str,
    scopes: &str,
    extra_config: &str,
) -> Result<(), AppError> {
    if resolve_provider(provider).is_none() {
        return Err(AppError::BadRequest("알 수 없는 OAuth 제공자입니다.".into()));
    }
    let clean_extra = extra_config.trim();
    let final_extra = if clean_extra.is_empty() { "{}" } else { clean_extra };
    if serde_json::from_str::<serde_json::Value>(final_extra).is_err() {
        return Err(AppError::BadRequest("extra_config는 유효한 JSON 형식이어야 합니다.".into()));
    }

    sqlx::query(
        "UPDATE oauth_providers
         SET is_enabled = ?, client_id = ?, client_secret = ?, scopes = ?, extra_config = ?, updated_at = CURRENT_TIMESTAMP
         WHERE provider = ?",
    )
    .bind(if is_enabled { 1_i64 } else { 0_i64 })
    .bind(client_id.trim())
    .bind(client_secret.trim())
    .bind(scopes.trim())
    .bind(final_extra)
    .bind(provider)
    .execute(pool)
    .await?;

    Ok(())
}

/// Complete social login (or link social account to currently logged-in user).
/// Returns Ok(Some((member, session_id))) on login, or Ok(None) when linking to existing logged-in session.
pub async fn complete_oauth_login_or_link(
    state: &AppState,
    current_member: Option<&Member>,
    provider_key: &str,
    display_name: &str,
    info: OAuthUserInfo,
    ip: &str,
    ua: &str,
) -> Result<Option<(Member, String)>, AppError> {
    let existing_link: Option<(i64,)> = sqlx::query_as(
        "SELECT member_id FROM member_oauth_links WHERE provider = ? AND provider_user_id = ?",
    )
    .bind(provider_key)
    .bind(&info.provider_user_id)
    .fetch_optional(&state.db)
    .await?;

    // Case 1: Logged-in user linking another social provider to their account
    if let Some(m) = current_member {
        if let Some((linked_mid,)) = existing_link {
            if linked_mid != m.id {
                return Err(AppError::BadRequest(
                    "이미 다른 회원 계정에 연결된 소셜 계정입니다.".into(),
                ));
            }
            return Ok(None);
        }
        sqlx::query(
            "INSERT INTO member_oauth_links (member_id, provider, provider_user_id, provider_email, provider_name)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(m.id)
        .bind(provider_key)
        .bind(&info.provider_user_id)
        .bind(&info.email)
        .bind(&info.nickname)
        .execute(&state.db)
        .await?;
        return Ok(None);
    }

    // Case 2: Guest logging in via already-linked social identity
    if let Some((member_id,)) = existing_link {
        let member = sqlx::query_as::<_, Member>(
            "SELECT * FROM members WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(member_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("연결된 회원 계정을 찾을 수 없습니다.".into()))?;

        let (updated, session_id, _, _) = auth_service::finalize_login_session(
            &state.db,
            &state.events,
            member,
            ip,
            ua,
            state.config.security.session_ttl_hours,
        )
        .await?;
        return Ok(Some((updated, session_id)));
    }

    // Case 3: First time seeing this social account -> auto-link by email if exists, else register new member
    let target_member = if let Some(ref mail) = info.email {
        let trimmed = mail.trim();
        if !trimmed.is_empty() {
            sqlx::query_as::<_, Member>(
                "SELECT * FROM members WHERE mb_email = ? AND deleted_at IS NULL",
            )
            .bind(trimmed)
            .fetch_optional(&state.db)
            .await?
        } else {
            None
        }
    } else {
        None
    };

    let member = if let Some(existing_member) = target_member {
        sqlx::query(
            "INSERT INTO member_oauth_links (member_id, provider, provider_user_id, provider_email, provider_name)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(existing_member.id)
        .bind(provider_key)
        .bind(&info.provider_user_id)
        .bind(&info.email)
        .bind(&info.nickname)
        .execute(&state.db)
        .await?;
        existing_member
    } else {
        let settings = settings_service::load_all(&state.db).await;
        if !settings.allow_register {
            return Err(AppError::Forbidden(
                "현재 신규 회원가입이 중단되어 있습니다.".into(),
            ));
        }

        let short_id = Uuid::new_v4().simple().to_string()[..8].to_string();
        let mb_id = format!("{}_{}", provider_key, short_id);
        let rand_pw = auth_service::generate_random_token();
        let pw_hash = auth_service::hash_password(&rand_pw)?;
        let mb_name = info
            .nickname
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(&mb_id)
            .to_string();
        let mb_email = info
            .email
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}@oauth.local", mb_id));

        let mut tx = state.db.begin().await?;
        let res = sqlx::query(
            "INSERT INTO members (mb_id, mb_password, mb_name, mb_email, mb_level, mb_point)
             VALUES (?, ?, ?, ?, ?, 0)",
        )
        .bind(&mb_id)
        .bind(pw_hash)
        .bind(&mb_name)
        .bind(&mb_email)
        .bind(settings.register_level)
        .execute(&mut *tx)
        .await?;

        let new_id = res.last_insert_rowid();

        sqlx::query(
            "INSERT INTO member_oauth_links (member_id, provider, provider_user_id, provider_email, provider_name)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(new_id)
        .bind(provider_key)
        .bind(&info.provider_user_id)
        .bind(&info.email)
        .bind(&info.nickname)
        .execute(&mut *tx)
        .await?;

        if settings.use_point && settings.register_point > 0 {
            let sys_lang = get_site_default_lang(&state.db).await;
            let msg = tr_sys(&sys_lang, "sys_msg.register_point", &[]);
            point_service::apply_point_in_tx(
                &mut tx,
                new_id,
                settings.register_point,
                Some("register"),
                Some(&new_id.to_string()),
                "register",
                &msg,
            )
            .await?;
        }

        tx.commit().await?;

        state.events.publish(DomainEvent::MemberJoined {
            member_id: new_id,
            mb_id: mb_id.clone(),
        });

        let sys_lang = get_site_default_lang(&state.db).await;
        let welcome = tr_sys(
            &sys_lang,
            "sys_msg.oauth_joined_noti",
            &[("provider", display_name)],
        );
        let _ = notification_service::dispatch_notification(
            &state.db,
            &state.events,
            new_id,
            "system",
            &welcome,
            Some("/member/profile"),
        )
        .await;

        sqlx::query_as::<_, Member>("SELECT * FROM members WHERE id = ?")
            .bind(new_id)
            .fetch_one(&state.db)
            .await?
    };

    let (updated, session_id, _, _) = auth_service::finalize_login_session(
        &state.db,
        &state.events,
        member,
        ip,
        ua,
        state.config.security.session_ttl_hours,
    )
    .await?;

    Ok(Some((updated, session_id)))
}
