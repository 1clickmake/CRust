use crate::error::AppError;
use crate::handlers::web::{build_base_context, PreviewQuery};
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::services::{ai, theme_service};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::{Form, Json};
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct AiSettingsForm {
    pub provider: String,
    pub api_key: Option<String>,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub endpoint: Option<String>,
    pub allowed_paths: String,
    pub allow_db_config: Option<String>,
    pub max_backups: i64,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct AiModelsLookupRequest {
    pub provider: String,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Deserialize)]
pub struct AiPlanRequest {
    pub prompt: String,
}

#[derive(Deserialize)]
pub struct AiApplyForm {
    pub plan_json: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct AiRollbackForm {
    pub backup_id: i64,
    pub csrf_token: Option<String>,
}

pub async fn admin_ai_settings_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let _admin = auth.require_admin()?;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (_theme, base) = build_base_context(&state, &auth, Some(csrf), None).await?;
    let mut cfg = ai::load_ai_settings(&state.db).await;
    let has_key = !cfg.api_key.trim().is_empty();
    // Never leak stored raw API key into HTML template
    cfg.api_key = String::new();

    let html = state.render_template(
        "admin/ai_settings.html",
        context! {
            ai_cfg => cfg,
            has_api_key => has_key,
            ..base
        },
    )?;
    Ok((jar, Html(html)))
}

pub async fn admin_ai_settings_save(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<AiSettingsForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    let prev = ai::load_ai_settings(&state.db).await;
    let submitted_key = form.api_key.unwrap_or_default();
    let final_key = if submitted_key.trim().is_empty() {
        prev.api_key
    } else {
        submitted_key.trim().to_string()
    };

    let next = ai::AiSettings {
        provider: form.provider,
        api_key: final_key,
        model: form.model,
        reasoning_effort: ai::normalize_reasoning_effort(
            form.reasoning_effort.as_deref().unwrap_or("default"),
        ),
        endpoint: form.endpoint.unwrap_or_default(),
        allowed_paths: form.allowed_paths,
        allow_db_config: form.allow_db_config.is_some(),
        max_backups: form.max_backups.clamp(1, 100),
    };
    ai::save_ai_settings(&state.db, &next).await?;
    Ok(Redirect::to("/admin/ai-settings?saved=1"))
}

pub async fn admin_ai_settings_models(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    Json(req): Json<AiModelsLookupRequest>,
) -> Result<Json<Value>, AppError> {
    let _admin = auth.require_admin()?;
    let csrf = headers.get("x-csrf-token").and_then(|v| v.to_str().ok());
    auth.verify_csrf(csrf)?;

    let prev = ai::load_ai_settings(&state.db).await;
    let provider = req.provider.trim().to_string();
    let submitted_key = req.api_key.unwrap_or_default();
    let effective_key = if !submitted_key.trim().is_empty() {
        submitted_key.trim().to_string()
    } else if prev.provider == provider {
        prev.api_key
    } else {
        String::new()
    };
    let submitted_ep = req.endpoint.unwrap_or_default();
    let effective_ep = if !submitted_ep.trim().is_empty() {
        submitted_ep.trim().to_string()
    } else if prev.provider == provider {
        prev.endpoint
    } else {
        String::new()
    };

    let temp_cfg = ai::AiSettings {
        provider: provider.clone(),
        api_key: effective_key,
        endpoint: effective_ep,
        ..ai::AiSettings::default()
    };

    let client = reqwest::Client::new();
    let models = ai::providers::fetch_available_models(&client, &temp_cfg)
        .await
        .map_err(|msg| AppError::ApiError(StatusCode::BAD_REQUEST, msg))?;

    Ok(Json(json!({
        "ok": true,
        "default_model": ai::providers::default_model_for_provider(&provider),
        "models": models
    })))
}

pub async fn admin_ai_builder_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let _admin = auth.require_admin()?;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (_theme, base) = build_base_context(&state, &auth, Some(csrf), None).await?;
    let cfg = ai::load_ai_settings(&state.db).await;
    let backups = ai::list_backups(&state.db).await?;

    let html = state.render_template(
        "admin/ai_builder.html",
        context! {
            ai_provider => cfg.provider,
            ai_model => cfg.model,
            ai_reasoning_effort => cfg.reasoning_effort,
            allowed_paths => cfg.allowed_paths,
            allow_db_config => cfg.allow_db_config,
            backups => backups,
            ..base
        },
    )?;
    Ok((jar, Html(html)))
}

pub async fn admin_ai_builder_plan(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    Json(req): Json<AiPlanRequest>,
) -> Result<Json<Value>, AppError> {
    let _admin = auth.require_admin()?;
    let csrf = headers.get("x-csrf-token").and_then(|v| v.to_str().ok());
    auth.verify_csrf(csrf)?;

    let plan_json = ai::generate_ai_plan(&state.db, &req.prompt).await?;
    Ok(Json(json!({ "ok": true, "plan_json": plan_json })))
}

pub async fn admin_ai_builder_apply(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<AiApplyForm>,
) -> Result<Redirect, AppError> {
    let admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;
    ai::apply_plan_with_snapshot(&state.db, admin.id, &form.plan_json).await?;
    let latest_theme = theme_service::load_active_theme(&state.db).await;
    *state.active_theme.write().await = latest_theme;
    Ok(Redirect::to("/admin/ai-builder?applied=1"))
}

pub async fn admin_ai_builder_rollback(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<AiRollbackForm>,
) -> Result<Redirect, AppError> {
    let _admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;
    ai::rollback_backup(&state.db, form.backup_id).await?;
    let latest_theme = theme_service::load_active_theme(&state.db).await;
    *state.active_theme.write().await = latest_theme;
    Ok(Redirect::to("/admin/ai-builder?rolled_back=1"))
}

pub async fn custom_page_view(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(slug): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Html<String>, AppError> {
    if !crate::state::is_safe_slug(&slug) {
        return Err(AppError::BadRequest("유효하지 않은 페이지 주소입니다.".into()));
    }
    let (theme, base) = build_base_context(&state, &auth, None, q.preview_theme.as_deref()).await?;

    // 1. 파일 기반 템플릿 우선 탐색 (templates/themes/{theme}/page/{slug}.html 또는 default Fallback)
    let page_rel = format!("page/{}.html", slug);
    let resolved_tpl = theme_service::resolve_template(&theme, &page_rel);
    if std::path::Path::new("templates").join(&resolved_tpl).exists() {
        let html = state.render_template(
            &resolved_tpl,
            context! {
                slug => slug,
                ..base
            },
        )?;
        return Ok(Html(html));
    }

    // 2. DB 기반 커스텀 페이지 조회
    let page = ai::get_published_page(&state.db, &slug).await?;
    let tpl = theme_service::resolve_template(&theme, "page.html");
    let html = state.render_template(
        &tpl,
        context! {
            page => page,
            ..base
        },
    )?;
    Ok(Html(html))
}
