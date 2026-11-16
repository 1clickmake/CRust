use crate::error::AppError;
use crate::handlers::web::build_base_context;
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::services::{memo_service, settings_service, theme_service};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct MemoPageQuery {
    pub tab: Option<String>,
    pub view: Option<i64>,
    pub to: Option<String>,
}

#[derive(Deserialize)]
pub struct MemoSendForm {
    pub receiver_mb_id: String,
    pub me_subject: String,
    pub me_content: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct MemoDeleteForm {
    pub csrf_token: Option<String>,
}

pub async fn memo_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Query(query): Query<MemoPageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let member = auth.require_member()?;
    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) = build_base_context(&state, &auth, Some(csrf), None).await?;

    let tab = query.tab.as_deref().unwrap_or("inbox");
    let viewing = if let Some(vid) = query.view {
        memo_service::get_and_mark_read(&state.db, member.id, vid).await?
    } else {
        None
    };
    let inbox = memo_service::list_inbox(&state.db, member.id).await?;
    let sent = memo_service::list_sent(&state.db, member.id).await?;

    let settings = settings_service::load_all(&state.db).await;
    let memo_cost = if member.mb_level >= 10 || !settings.use_point {
        0
    } else {
        settings.memo_send_point
    };

    let tpl = theme_service::resolve_template(&theme, "member/memo.html");
    let html = state.render_template(
        &tpl,
        context! {
            tab => tab,
            inbox => inbox,
            sent => sent,
            viewing => viewing,
            prefill_to => query.to.unwrap_or_default(),
            memo_cost => memo_cost,
            ..base
        },
    )?;

    Ok((jar, Html(html)))
}

pub async fn memo_send(
    State(state): State<AppState>,
    auth: AuthContext,
    Form(form): Form<MemoSendForm>,
) -> Result<Redirect, AppError> {
    let member = auth.require_member()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    memo_service::send_memo(
        &state,
        member,
        &form.receiver_mb_id,
        &form.me_subject,
        &form.me_content,
    )
    .await?;

    Ok(Redirect::to("/member/memo?tab=sent"))
}

pub async fn memo_delete(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(memo_id): Path<i64>,
    Form(form): Form<MemoDeleteForm>,
) -> Result<Redirect, AppError> {
    let member = auth.require_member()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;

    memo_service::delete_memo(&state.db, member.id, memo_id).await?;
    Ok(Redirect::to("/member/memo"))
}
