use crate::error::AppError;
use crate::events::{DomainEvent, EventBus};
use crate::models::member::Member;
use crate::services::{point_service, settings_service};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use sqlx::SqlitePool;

pub fn hash_password(plain: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hash error: {}", e)))
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

pub fn generate_random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub async fn get_site_config_i64(pool: &SqlitePool, key: &str, default: i64) -> i64 {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT cf_value FROM site_configs WHERE cf_key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    row.and_then(|(v,)| v.parse::<i64>().ok()).unwrap_or(default)
}

pub async fn register_member(
    pool: &SqlitePool,
    events: &EventBus,
    mb_id: &str,
    plain_pw: &str,
    mb_name: &str,
    mb_email: &str,
) -> Result<i64, AppError> {
    let settings = settings_service::load_all(pool).await;
    if !settings.allow_register {
        return Err(AppError::Forbidden(
            "현재 신규 회원가입이 중단되어 있습니다.".into(),
        ));
    }

    let mb_id = mb_id.trim();
    let mb_name = mb_name.trim();
    let mb_email = mb_email.trim();

    if mb_id.len() < 3 || mb_id.len() > 30 || !mb_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::BadRequest(
            "아이디는 3~30자의 영문, 숫자, 밑줄(_)만 사용할 수 있습니다.".into(),
        ));
    }
    if plain_pw.len() < 4 {
        return Err(AppError::BadRequest("비밀번호는 4자 이상이어야 합니다.".into()));
    }
    if mb_name.is_empty() || mb_email.is_empty() || !mb_email.contains('@') {
        return Err(AppError::BadRequest("닉네임과 올바른 이메일 주소를 입력해주세요.".into()));
    }

    for raw in settings.prohibit_id.split(',') {
        let banned = raw.trim().to_lowercase();
        if !banned.is_empty()
            && (mb_id.to_lowercase() == banned || mb_name.to_lowercase() == banned)
        {
            return Err(AppError::BadRequest(
                "사용할 수 없는 예약어/금지 아이디 또는 닉네임입니다.".into(),
            ));
        }
    }

    let existing: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM members WHERE mb_id = ? OR mb_email = ?",
    )
    .bind(mb_id)
    .bind(mb_email)
    .fetch_one(pool)
    .await?;

    if existing.0 > 0 {
        return Err(AppError::BadRequest("이미 사용 중인 아이디 또는 이메일입니다.".into()));
    }

    let pw_hash = hash_password(plain_pw)?;
    let default_level = settings.register_level;
    let use_point = settings.use_point;
    let reg_point = settings.register_point;

    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "INSERT INTO members (mb_id, mb_password, mb_name, mb_email, mb_level, mb_point)
         VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind(mb_id)
    .bind(pw_hash)
    .bind(mb_name)
    .bind(mb_email)
    .bind(default_level)
    .execute(&mut *tx)
    .await?;

    let member_id = res.last_insert_rowid();

    if use_point && reg_point > 0 {
        let sys_lang = crate::i18n::get_site_default_lang(pool).await;
        let msg = crate::i18n::tr_sys(&sys_lang, "sys_msg.register_point", &[]);
        point_service::apply_point_in_tx(
            &mut tx,
            member_id,
            reg_point,
            Some("register"),
            Some(&member_id.to_string()),
            "register",
            &msg,
        )
        .await?;
    }

    tx.commit().await?;

    events.publish(DomainEvent::MemberJoined {
        member_id,
        mb_id: mb_id.to_string(),
    });

    Ok(member_id)
}

pub async fn login_member(
    pool: &SqlitePool,
    events: &EventBus,
    mb_id: &str,
    plain_pw: &str,
    ip: &str,
    user_agent: &str,
    ttl_hours: i64,
) -> Result<(Member, String, String, i64), AppError> {
    let member = sqlx::query_as::<_, Member>(
        "SELECT * FROM members WHERE mb_id = ? AND deleted_at IS NULL",
    )
    .bind(mb_id.trim())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("아이디 또는 비밀번호가 일치하지 않습니다.".into()))?;

    if !verify_password(plain_pw, &member.mb_password) {
        return Err(AppError::Unauthorized(
            "아이디 또는 비밀번호가 일치하지 않습니다.".into(),
        ));
    }

    if member.is_blocked != 0 {
        return Err(AppError::Forbidden("차단된 계정입니다. 관리자에게 문의하세요.".into()));
    }

    finalize_login_session(pool, events, member, ip, user_agent, ttl_hours).await
}

/// Shared session creation + daily login point bonus + Login EventBus dispatch.
/// Reused by both password login and OAuth2 social login.
pub async fn finalize_login_session(
    pool: &SqlitePool,
    events: &EventBus,
    member: Member,
    ip: &str,
    user_agent: &str,
    ttl_hours: i64,
) -> Result<(Member, String, String, i64), AppError> {
    if member.is_blocked != 0 {
        return Err(AppError::Forbidden("차단된 계정입니다. 관리자에게 문의하세요.".into()));
    }

    let login_point_cfg = get_site_config_i64(pool, "login_point", 100).await;
    let use_point = point_service::is_point_enabled(pool).await;

    let mut tx = pool.begin().await?;

    // Atomic daily login bonus check
    let mut awarded_point = 0;
    let daily_res = sqlx::query(
        "UPDATE members
         SET mb_today_login = CURRENT_TIMESTAMP, mb_login_ip = ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ? AND (mb_today_login IS NULL OR DATE(mb_today_login, 'localtime') < DATE('now', 'localtime'))",
    )
    .bind(ip)
    .bind(member.id)
    .execute(&mut *tx)
    .await?;

    if daily_res.rows_affected() == 1 && use_point && login_point_cfg > 0 {
        let sys_lang = crate::i18n::get_site_default_lang(pool).await;
        let msg = crate::i18n::tr_sys(&sys_lang, "sys_msg.login_point", &[]);
        point_service::apply_point_in_tx(
            &mut tx,
            member.id,
            login_point_cfg,
            Some("login"),
            Some(&Utc::now().format("%Y-%m-%d").to_string()),
            "login",
            &msg,
        )
        .await?;
        awarded_point = login_point_cfg;
    } else {
        sqlx::query("UPDATE members SET mb_login_ip = ? WHERE id = ?")
            .bind(ip)
            .bind(member.id)
            .execute(&mut *tx)
            .await?;
    }

    let session_id = generate_random_token();
    let csrf_token = generate_random_token();
    let expires_at = (Utc::now() + Duration::hours(ttl_hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    sqlx::query(
        "INSERT INTO sessions (id, member_id, csrf_token, ip_address, user_agent, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(member.id)
    .bind(&csrf_token)
    .bind(ip)
    .bind(user_agent)
    .bind(&expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if awarded_point > 0 {
        events.publish(DomainEvent::PointChanged {
            member_id: member.id,
            delta: awarded_point,
            balance_after: member.mb_point + awarded_point,
            action: "login".into(),
        });
    }

    let updated_member = sqlx::query_as::<_, Member>("SELECT * FROM members WHERE id = ?")
        .bind(member.id)
        .fetch_one(pool)
        .await?;

    Ok((updated_member, session_id, csrf_token, awarded_point))
}

pub async fn get_session_member(
    pool: &SqlitePool,
    session_id: &str,
) -> Option<(Member, String)> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT member_id, csrf_token FROM sessions
         WHERE id = ? AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (member_id, csrf_token) = row?;

    let member = sqlx::query_as::<_, Member>(
        "SELECT * FROM members WHERE id = ? AND is_blocked = 0 AND deleted_at IS NULL",
    )
    .bind(member_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    Some((member, csrf_token))
}

pub async fn logout_session(pool: &SqlitePool, session_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}
