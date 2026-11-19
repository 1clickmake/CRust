use crate::error::AppError;
use crate::handlers::web::{build_base_context, PreviewQuery};
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::models::member::MemberLevelConfig;
use crate::models::point::PointLog;
use crate::models::post::Post;
use crate::services::{auth_service, notification_service, oauth, settings_service, theme_service};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    pub mb_id: String,
    pub mb_password: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub mb_id: String,
    pub mb_password: String,
    pub mb_name: String,
    pub mb_email: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub csrf_token: Option<String>,
}

pub async fn login_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Query(q): Query<PreviewQuery>,
) -> Result<impl IntoResponse, AppError> {
    if auth.member.is_some() {
        return Ok((jar, Redirect::to("/")).into_response());
    }
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) =
        build_base_context(&state, &auth, Some(csrf), q.preview_theme.as_deref()).await?;

    let tpl = theme_service::resolve_template(&theme, "member/login.html");
    let enabled_oauth = oauth::list_enabled_providers(&state.db).await;
    let html = state.render_template(
        &tpl,
        context! {
            enabled_oauth => enabled_oauth,
            meta_robots => "noindex, follow",
            ..base
        },
    )?;
    Ok((jar, Html(html)).into_response())
}

pub async fn login_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let (_member, session_id, _csrf, _awarded) = auth_service::login_member(
        &state.db,
        &state.events,
        &form.mb_id,
        &form.mb_password,
        &auth.client_ip,
        &auth.user_agent,
        state.config.security.session_ttl_hours,
    )
    .await?;

    let cookie = Cookie::build((
        state.config.security.session_cookie_name.clone(),
        session_id,
    ))
    .path("/")
    .http_only(true)
    .secure(state.config.security.cookie_secure)
    .same_site(SameSite::Lax)
    .build();

    Ok((jar.add(cookie), Redirect::to("/")))
}

pub async fn register_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Query(q): Query<PreviewQuery>,
) -> Result<impl IntoResponse, AppError> {
    if auth.member.is_some() {
        return Ok((jar, Redirect::to("/")).into_response());
    }
    let settings = settings_service::load_all(&state.db).await;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) =
        build_base_context(&state, &auth, Some(csrf), q.preview_theme.as_deref()).await?;

    let tpl = theme_service::resolve_template(&theme, "member/register.html");
    let html = state.render_template(
        &tpl,
        context! {
            allow_register => settings.allow_register,
            stipulation => settings.stipulation,
            privacy => settings.privacy,
            ..base
        },
    )?;
    Ok((jar, Html(html)).into_response())
}

pub async fn register_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<RegisterForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;

    auth_service::register_member(
        &state.db,
        &state.events,
        &form.mb_id,
        &form.mb_password,
        &form.mb_name,
        &form.mb_email,
    )
    .await?;

    Ok(Redirect::to("/member/login"))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> Result<impl IntoResponse, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    if let Some(sid) = auth.session_id {
        let _ = auth_service::logout_session(&state.db, &sid).await;
    }
    let removal = Cookie::build((state.config.security.session_cookie_name.clone(), ""))
        .path("/")
        .build();
    Ok((jar.remove(removal), Redirect::to("/")))
}

#[derive(Deserialize, Default)]
pub struct ProfileQuery {
    pub preview_theme: Option<String>,
    pub pt_filter: Option<String>,
    pub msg: Option<String>,
}

#[derive(Deserialize)]
pub struct ProfileUpdateForm {
    pub mb_name: String,
    pub mb_email: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct PasswordUpdateForm {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
    pub csrf_token: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct MyCommentItem {
    pub id: i64,
    pub post_id: i64,
    pub bo_table: String,
    pub wr_subject: String,
    pub co_content: String,
    pub created_at: String,
}

pub async fn profile_page(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(q): Query<ProfileQuery>,
) -> Result<Html<String>, AppError> {
    let member = auth.require_member()?;
    let (_theme, base) =
        build_base_context(&state, &auth, None, q.preview_theme.as_deref()).await?;

    let pt_filter = match q.pt_filter.as_deref() {
        Some("plus") => "plus",
        Some("minus") => "minus",
        _ => "all",
    };

    let point_sql = match pt_filter {
        "plus" => "SELECT * FROM point_logs WHERE member_id = ? AND point > 0 ORDER BY id DESC LIMIT 50",
        "minus" => "SELECT * FROM point_logs WHERE member_id = ? AND point < 0 ORDER BY id DESC LIMIT 50",
        _ => "SELECT * FROM point_logs WHERE member_id = ? ORDER BY id DESC LIMIT 50",
    };
    let point_logs = sqlx::query_as::<_, PointLog>(point_sql)
        .bind(member.id)
        .fetch_all(&state.db)
        .await?;

    let my_posts = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE member_id = ? AND deleted_at IS NULL ORDER BY id DESC LIMIT 30",
    )
    .bind(member.id)
    .fetch_all(&state.db)
    .await?;

    let my_comments = sqlx::query_as::<_, MyCommentItem>(
        "SELECT c.id, c.post_id, p.bo_table, p.wr_subject, c.co_content, c.created_at
         FROM comments c
         JOIN posts p ON c.post_id = p.id
         WHERE c.member_id = ? AND c.deleted_at IS NULL AND p.deleted_at IS NULL
         ORDER BY c.id DESC LIMIT 30",
    )
    .bind(member.id)
    .fetch_all(&state.db)
    .await?;

    let level_info = sqlx::query_as::<_, MemberLevelConfig>(
        "SELECT * FROM member_level_configs WHERE level = ?",
    )
    .bind(member.mb_level)
    .fetch_optional(&state.db)
    .await?;

    let notifications = notification_service::list_for_member(&state.db, member.id, 20).await?;
    let oauth_links = oauth::list_member_links(&state.db, member.id).await;
    let enabled_oauth = oauth::list_enabled_providers(&state.db).await;

    sqlx::query("UPDATE notifications SET is_read = 1 WHERE member_id = ?")
        .bind(member.id)
        .execute(&state.db)
        .await?;

    let tpl = theme_service::resolve_template(&_theme, "member/profile.html");
    let html = state.render_template(
        &tpl,
        context! {
            point_logs => point_logs,
            pt_filter => pt_filter,
            my_posts => my_posts,
            my_comments => my_comments,
            level_info => level_info,
            notifications => notifications,
            oauth_links => oauth_links,
            enabled_oauth => enabled_oauth,
            status_msg => q.msg.unwrap_or_default(),
            ..base
        },
    )?;

    Ok(Html(html))
}

pub async fn profile_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<ProfileUpdateForm>,
) -> Result<Redirect, AppError> {
    let member = auth.require_member()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let mb_name = form.mb_name.trim();
    let mb_email = form.mb_email.trim();
    if mb_name.is_empty() || mb_email.is_empty() || !mb_email.contains('@') {
        return Err(AppError::BadRequest(
            "올바른 닉네임과 이메일 주소를 입력해주세요.".into(),
        ));
    }

    if member.mb_level < 10 {
        let settings = settings_service::load_all(&state.db).await;
        for raw in settings.prohibit_id.split(',') {
            let banned = raw.trim().to_lowercase();
            if !banned.is_empty() && mb_name.to_lowercase() == banned {
                return Err(AppError::BadRequest(
                    "사용할 수 없는 예약어/금지 닉네임입니다.".into(),
                ));
            }
        }
    }

    let dup: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM members WHERE mb_email = ? AND id != ?")
            .bind(mb_email)
            .bind(member.id)
            .fetch_one(&state.db)
            .await?;
    if dup.0 > 0 {
        return Err(AppError::BadRequest("이미 사용 중인 이메일입니다.".into()));
    }

    sqlx::query(
        "UPDATE members SET mb_name = ?, mb_email = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(mb_name)
    .bind(mb_email)
    .bind(member.id)
    .execute(&state.db)
    .await?;

    Ok(Redirect::to("/member/profile?msg=profile_updated"))
}

pub async fn profile_password_update(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<PasswordUpdateForm>,
) -> Result<Redirect, AppError> {
    let member = auth.require_member()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    if !auth_service::verify_password(&form.current_password, &member.mb_password) {
        return Err(AppError::Forbidden("현재 비밀번호가 일치하지 않습니다.".into()));
    }
    if form.new_password.len() < 4 {
        return Err(AppError::BadRequest(
            "새 비밀번호는 4자 이상이어야 합니다.".into(),
        ));
    }
    if form.new_password != form.confirm_password {
        return Err(AppError::BadRequest(
            "새 비밀번호와 확인 비밀번호가 일치하지 않습니다.".into(),
        ));
    }

    let new_hash = auth_service::hash_password(&form.new_password)?;
    sqlx::query(
        "UPDATE members SET mb_password = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(new_hash)
    .bind(member.id)
    .execute(&state.db)
    .await?;

    Ok(Redirect::to("/member/profile?msg=password_updated"))
}
