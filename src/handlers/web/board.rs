use crate::error::AppError;
use crate::handlers::web::build_base_context;
use crate::middlewares::auth::{ensure_csrf_cookie, AuthContext};
use crate::services::{board_service, settings_service, theme_service};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use axum_extra::extract::CookieJar;
use minijinja::context;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub q: Option<String>,
    pub category: Option<String>,
    pub preview_theme: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct PreviewOnlyQuery {
    pub preview_theme: Option<String>,
}

#[derive(Deserialize)]
pub struct PostWriteForm {
    pub wr_name: Option<String>,
    pub wr_password: Option<String>,
    pub wr_category: Option<String>,
    pub wr_subject: String,
    pub wr_content: String,
    pub is_notice: Option<String>,
    pub is_secret: Option<String>,
    pub wr_link1: Option<String>,
    pub wr_link2: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct PostEditForm {
    pub wr_password: Option<String>,
    pub wr_category: Option<String>,
    pub wr_subject: String,
    pub wr_content: String,
    pub is_notice: Option<String>,
    pub is_secret: Option<String>,
    pub wr_link1: Option<String>,
    pub wr_link2: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct CommentForm {
    pub co_name: Option<String>,
    pub co_password: Option<String>,
    pub co_content: String,
    pub parent_id: Option<String>,
    pub is_secret: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct CommentEditForm {
    pub co_password: Option<String>,
    pub co_content: String,
    pub is_secret: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct CommentDeleteForm {
    pub co_password: Option<String>,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    pub wr_password: Option<String>,
    pub csrf_token: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(bo_table): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Html<String>, AppError> {
    let board = board_service::get_board(&state, &bo_table).await?;
    if auth.level() < board.bo_list_level {
        return Err(AppError::Forbidden(format!(
            "게시판 목록 보기 권한이 없습니다. (필요 레벨: {})",
            board.bo_list_level
        )));
    }

    let (theme, base) =
        build_base_context(&state, &auth, None, query.preview_theme.as_deref()).await?;

    let settings = settings_service::load_all(&state.db).await;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = settings.page_rows;
    let offset = (page - 1) * per_page;

    let (posts, total_count) = board_service::list_posts(
        &state,
        &bo_table,
        query.q.as_deref(),
        query.category.as_deref(),
        per_page,
        offset,
    )
    .await?;

    let total_pages = ((total_count as f64) / (per_page as f64)).ceil().max(1.0) as i64;
    let categories = board.categories();
    let post_items = board_service::build_post_list_items(&state.db, posts).await;
    let tpl = theme_service::resolve_board_template(&theme, &board.bo_skin, "list.html");

    let html = state.render_template(
        &tpl,
        context! {
            board => board,
            categories => categories,
            posts => post_items,
            page => page,
            per_page => per_page,
            offset => offset,
            total_pages => total_pages,
            total_count => total_count,
            search_q => query.q.unwrap_or_default(),
            current_category => query.category.unwrap_or_default(),
            ..base
        },
    )?;

    Ok(Html(html))
}

pub async fn view(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Path((bo_table, post_id)): Path<(String, i64)>,
    Query(query): Query<PreviewOnlyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let board = board_service::get_board(&state, &bo_table).await?;
    let (post, comments) =
        board_service::view_post(&state, &board, post_id, auth.member.as_ref()).await?;

    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) =
        build_base_context(&state, &auth, Some(csrf), query.preview_theme.as_deref()).await?;

    let is_guest_post = auth.member.is_none() && post.member_id.is_none();
    let can_manage = auth
        .member
        .as_ref()
        .map(|m| m.mb_level >= 10 || post.member_id == Some(m.id))
        .unwrap_or(false)
        || (is_guest_post && post.is_secret == 0);

    let comment_views = board_service::build_comment_views(comments.clone(), &post, auth.member.as_ref());
    let post_extra: serde_json::Value =
        serde_json::from_str(&post.extra_data).unwrap_or_else(|_| serde_json::json!({}));
    let og_title = post.wr_subject.clone();
    let og_description = board_service::strip_html_summary(&post.wr_content, 150);
    let og_image = board_service::extract_first_image(&post.wr_content).unwrap_or_default();

    let tpl = theme_service::resolve_board_template(&theme, &board.bo_skin, "view.html");
    let html = state.render_template(
        &tpl,
        context! {
            board => board,
            post => post,
            comments => comment_views,
            post_extra => post_extra,
            og_title => og_title,
            og_description => og_description,
            og_image => og_image,
            og_type => "article",
            can_edit => can_manage,
            can_delete => can_manage,
            is_guest_post => is_guest_post,
            ..base
        },
    )?;

    Ok((jar, Html(html)))
}

pub async fn write_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Path(bo_table): Path<String>,
    Query(query): Query<PreviewOnlyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let board = board_service::get_board(&state, &bo_table).await?;
    if auth.level() < board.bo_write_level {
        return Err(AppError::Forbidden(format!(
            "글쓰기 권한이 없습니다. (필요 레벨: {})",
            board.bo_write_level
        )));
    }

    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) =
        build_base_context(&state, &auth, Some(csrf), query.preview_theme.as_deref()).await?;

    let categories = board.categories();
    let tpl = theme_service::resolve_board_template(&theme, &board.bo_skin, "write.html");
    let html = state.render_template(
        &tpl,
        context! {
            board => board,
            categories => categories,
            ..base
        },
    )?;

    Ok((jar, Html(html)))
}

pub async fn write_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(bo_table): Path<String>,
    Form(form): Form<PostWriteForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;

    let post_id = board_service::create_post(
        &state,
        &board,
        auth.member.as_ref(),
        form.wr_name.as_deref().unwrap_or(""),
        form.wr_password.as_deref(),
        form.wr_category.as_deref(),
        &form.wr_subject,
        &form.wr_content,
        form.is_notice.is_some(),
        form.is_secret.is_some(),
        form.wr_link1.as_deref(),
        form.wr_link2.as_deref(),
        &auth.client_ip,
    )
    .await?;

    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn edit_page(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Path((bo_table, post_id)): Path<(String, i64)>,
    Query(query): Query<PreviewOnlyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let board = board_service::get_board(&state, &bo_table).await?;
    let post = board_service::get_post_for_edit(&state, &board, post_id, auth.member.as_ref()).await?;

    let (jar, csrf) = ensure_csrf_cookie(&auth, jar);
    let (theme, base) =
        build_base_context(&state, &auth, Some(csrf), query.preview_theme.as_deref()).await?;

    let need_guest_password = auth.member.is_none() && post.member_id.is_none();
    let categories = board.categories();
    let tpl = theme_service::resolve_board_template(&theme, &board.bo_skin, "edit.html");
    let html = state.render_template(
        &tpl,
        context! {
            board => board,
            post => post,
            categories => categories,
            need_guest_password => need_guest_password,
            ..base
        },
    )?;

    Ok((jar, Html(html)))
}

pub async fn edit_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id)): Path<(String, i64)>,
    Form(form): Form<PostEditForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;

    board_service::update_post(
        &state,
        &board,
        post_id,
        auth.member.as_ref(),
        form.wr_password.as_deref(),
        form.wr_category.as_deref(),
        &form.wr_subject,
        &form.wr_content,
        form.is_notice.is_some(),
        form.is_secret.is_some(),
        form.wr_link1.as_deref(),
        form.wr_link2.as_deref(),
    )
    .await?;

    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn comment_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id)): Path<(String, i64)>,
    Form(form): Form<CommentForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;
    let (post, _) =
        board_service::view_post(&state, &board, post_id, auth.member.as_ref()).await?;

    let parent_id = match form.parent_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(
            raw.parse::<i64>()
                .map_err(|_| AppError::BadRequest("유효하지 않은 부모 댓글 ID입니다.".into()))?,
        ),
        None => None,
    };

    board_service::create_comment(
        &state,
        &board,
        &post,
        auth.member.as_ref(),
        form.co_name.as_deref().unwrap_or(""),
        form.co_password.as_deref(),
        &form.co_content,
        parent_id,
        form.is_secret.is_some(),
        &auth.client_ip,
    )
    .await?;

    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn comment_edit_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id, comment_id)): Path<(String, i64, i64)>,
    Form(form): Form<CommentEditForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;
    board_service::update_comment(
        &state,
        &board,
        post_id,
        comment_id,
        auth.member.as_ref(),
        form.co_password.as_deref(),
        &form.co_content,
        form.is_secret.is_some(),
    )
    .await?;
    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn comment_delete_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id, comment_id)): Path<(String, i64, i64)>,
    Form(form): Form<CommentDeleteForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;
    board_service::soft_delete_comment(
        &state,
        &board,
        post_id,
        comment_id,
        auth.member.as_ref(),
        form.co_password.as_deref(),
    )
    .await?;
    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn qna_accept_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id, comment_id)): Path<(String, i64, i64)>,
    Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;
    board_service::accept_qna_answer(&state, &board, post_id, comment_id, auth.member.as_ref()).await?;
    Ok(Redirect::to(&format!("/board/{}/{}", bo_table, post_id)))
}

pub async fn delete_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((bo_table, post_id)): Path<(String, i64)>,
    Form(form): Form<DeleteForm>,
) -> Result<Redirect, AppError> {
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;

    board_service::soft_delete_post(
        &state,
        &board,
        post_id,
        auth.member.as_ref(),
        form.wr_password.as_deref(),
    )
    .await?;
    Ok(Redirect::to(&format!("/board/{}", bo_table)))
}

pub async fn bulk_delete_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(bo_table): Path<String>,
    Form(form): Form<crate::handlers::web::admin::BulkIdsForm>,
) -> Result<Redirect, AppError> {
    let admin = auth.require_admin()?;
    auth.verify_csrf(form.csrf_token.as_deref())?;
    let board = board_service::get_board(&state, &bo_table).await?;

    for id in crate::handlers::web::admin::parse_ids_csv(form.ids_csv.as_deref()) {
        let _ = board_service::soft_delete_post(&state, &board, id, Some(admin), None).await;
    }

    Ok(Redirect::to(&format!("/board/{}", bo_table)))
}
