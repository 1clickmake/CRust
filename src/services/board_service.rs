use crate::error::AppError;
use crate::events::DomainEvent;
use crate::models::board::Board;
use crate::models::member::Member;
use crate::models::post::{Comment, CommentView, Post, PostListItem};
use crate::services::{auth_service, file_service, level_service, notification_service, point_service, settings_service};
use crate::state::AppState;
use ammonia::{Builder, UrlRelative};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Sanitize Asamiya-Editor HTML safely while preserving formatting, YouTube embeds, and file cards
pub fn sanitize_editor_html(raw_html: &str) -> String {
    let mut tags: HashSet<&str> = HashSet::from([
        "p", "br", "b", "strong", "i", "em", "u", "s", "strike", "del",
        "h1", "h2", "h3", "h4", "h5", "h6",
        "blockquote", "pre", "code", "hr",
        "ul", "ol", "li",
        "table", "thead", "tbody", "tr", "th", "td",
        "a", "img", "div", "span", "font", "iframe",
    ]);

    let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    tag_attrs.insert("a", HashSet::from(["href", "target", "class"]));
    tag_attrs.insert("img", HashSet::from(["src", "alt", "title", "width", "height", "style"]));
    tag_attrs.insert("div", HashSet::from(["class", "style", "data-file-uuid", "contenteditable"]));
    tag_attrs.insert("span", HashSet::from(["class", "style"]));
    tag_attrs.insert("p", HashSet::from(["style"]));
    tag_attrs.insert("font", HashSet::from(["style", "color", "face"]));
    tag_attrs.insert("i", HashSet::from(["class"]));
    tag_attrs.insert("table", HashSet::from(["class"]));
    tag_attrs.insert("iframe", HashSet::from(["width", "height", "src", "frameborder", "allow", "allowfullscreen"]));

    let url_schemes: HashSet<&str> = HashSet::from(["http", "https", "mailto"]);

    let mut builder = Builder::empty();
    builder
        .tags(tags.drain().collect())
        .tag_attributes(tag_attrs)
        .url_schemes(url_schemes)
        .url_relative(UrlRelative::PassThrough)
        .attribute_filter(|element, attribute, value| {
            // Only permit YouTube embeds in <iframe>
            if element == "iframe" && attribute == "src" {
                if value.starts_with("https://www.youtube.com/embed/") {
                    return Some(Cow::Borrowed(value));
                }
                return None;
            }
            // Prevent dangerous CSS payloads inside inline style attributes
            if attribute == "style" {
                let lower = value.to_ascii_lowercase();
                if lower.contains("expression(")
                    || lower.contains("javascript:")
                    || lower.contains("url(")
                    || lower.contains("-moz-binding")
                {
                    return None;
                }
                return Some(Cow::Borrowed(value));
            }
            Some(Cow::Borrowed(value))
        });

    builder.clean(raw_html).to_string()
}

pub async fn get_board(state: &AppState, bo_table: &str) -> Result<Board, AppError> {
    sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE bo_table = ?")
        .bind(bo_table)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("존재하지 않는 게시판({})입니다.", bo_table)))
}

pub async fn list_posts(
    state: &AppState,
    bo_table: &str,
    keyword: Option<&str>,
    category: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Post>, i64), AppError> {
    let kw = keyword.map(|s| s.trim()).filter(|s| !s.is_empty());
    let cat = category.map(|s| s.trim()).filter(|s| !s.is_empty());

    if let Some(q) = kw {
        // Use FTS5 Trigram search when query has >= 3 chars, fallback to LIKE for 1~2 char queries
        if q.chars().count() >= 3 {
            let escaped_q = format!("\"{}\"", q.replace('"', "\"\""));
            let posts = sqlx::query_as::<_, Post>(
                "SELECT p.* FROM posts p
                 JOIN posts_fts f ON p.id = f.rowid
                 WHERE p.bo_table = ? AND p.deleted_at IS NULL AND posts_fts MATCH ?
                 ORDER BY p.is_notice DESC, p.id DESC LIMIT ? OFFSET ?",
            )
            .bind(bo_table)
            .bind(&escaped_q)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?;

            let (cnt,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM posts p
                 JOIN posts_fts f ON p.id = f.rowid
                 WHERE p.bo_table = ? AND p.deleted_at IS NULL AND posts_fts MATCH ?",
            )
            .bind(bo_table)
            .bind(&escaped_q)
            .fetch_one(&state.db)
            .await?;

            return Ok((posts, cnt));
        } else {
            let pattern = format!("%{}%", q);
            let posts = sqlx::query_as::<_, Post>(
                "SELECT * FROM posts
                 WHERE bo_table = ? AND deleted_at IS NULL
                   AND (wr_subject LIKE ? OR wr_content LIKE ?)
                 ORDER BY is_notice DESC, id DESC LIMIT ? OFFSET ?",
            )
            .bind(bo_table)
            .bind(&pattern)
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?;

            let (cnt,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM posts
                 WHERE bo_table = ? AND deleted_at IS NULL
                   AND (wr_subject LIKE ? OR wr_content LIKE ?)",
            )
            .bind(bo_table)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_one(&state.db)
            .await?;

            return Ok((posts, cnt));
        }
    }

    if let Some(c) = cat {
        let posts = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts
             WHERE bo_table = ? AND deleted_at IS NULL AND wr_category = ?
             ORDER BY is_notice DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(bo_table)
        .bind(c)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let (cnt,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM posts
             WHERE bo_table = ? AND deleted_at IS NULL AND wr_category = ?",
        )
        .bind(bo_table)
        .bind(c)
        .fetch_one(&state.db)
        .await?;

        return Ok((posts, cnt));
    }

    let posts = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts
         WHERE bo_table = ? AND deleted_at IS NULL
         ORDER BY is_notice DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(bo_table)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let (cnt,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE bo_table = ? AND deleted_at IS NULL",
    )
    .bind(bo_table)
    .fetch_one(&state.db)
    .await?;

    Ok((posts, cnt))
}

pub async fn view_post(
    state: &AppState,
    board: &Board,
    post_id: i64,
    member: Option<&Member>,
) -> Result<(Post, Vec<Comment>), AppError> {
    let post = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE id = ? AND bo_table = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(&board.bo_table)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("게시글을 찾을 수 없습니다.".into()))?;

    let user_level = member.map(|m| m.mb_level).unwrap_or(1);
    if user_level < board.bo_read_level {
        return Err(AppError::Forbidden(format!(
            "글 읽기 권한이 없습니다. (필요 레벨: {})",
            board.bo_read_level
        )));
    }

    if post.is_secret != 0 {
        let allowed = member
            .map(|m| m.mb_level >= 10 || post.member_id == Some(m.id))
            .unwrap_or(false);
        if !allowed {
            return Err(AppError::Forbidden("비밀글은 작성자와 관리자만 열람할 수 있습니다.".into()));
        }
    }

    if board.bo_read_point != 0 && point_service::is_point_enabled(&state.db).await {
        if let Some(m) = member {
            let is_owner = post.member_id == Some(m.id);
            if !is_owner && m.mb_level < 10 {
                let rel_id = format!("{}:{}", board.bo_table, post.id);
                let paid = point_service::has_point_log(
                    &state.db,
                    m.id,
                    "board",
                    &rel_id,
                    "view",
                )
                .await?;
                if !paid {
                    let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
                    let msg = crate::i18n::tr_sys(
                        &sys_lang,
                        "sys_msg.read_point",
                        &[("board", &board.bo_subject), ("id", &post.id.to_string())],
                    );
                    point_service::apply_point(
                        &state.db,
                        &state.events,
                        m.id,
                        board.bo_read_point,
                        Some("board"),
                        Some(&rel_id),
                        "view",
                        &msg,
                    )
                    .await?;
                }
            }
        } else if board.bo_read_point < 0 {
            return Err(AppError::Unauthorized(
                "포인트가 차감되는 글은 로그인 후 열람할 수 있습니다.".into(),
            ));
        }
    }

    sqlx::query("UPDATE posts SET wr_hit = wr_hit + 1 WHERE id = ?")
        .bind(post.id)
        .execute(&state.db)
        .await?;

    let comments = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE post_id = ? AND deleted_at IS NULL ORDER BY id ASC",
    )
    .bind(post.id)
    .fetch_all(&state.db)
    .await?;

    Ok((post, comments))
}

async fn check_write_policy(
    state: &AppState,
    user_level: i64,
    ip: &str,
    member_id: Option<i64>,
    text_to_check: &str,
    check_delay: bool,
    table: &str,
) -> Result<(), AppError> {
    if user_level >= 10 {
        return Ok(());
    }
    let settings = settings_service::load_all(&state.db).await;

    if let Some(word) = settings_service::find_filtered_word(&settings.filter_words, text_to_check) {
        return Err(AppError::BadRequest(format!(
            "금지어('{}')가 포함되어 작성할 수 없습니다.",
            word
        )));
    }

    if check_delay && settings.delay_sec > 0 {
        let ip_col = if table == "posts" { "wr_ip" } else { "co_ip" };
        let sql = if member_id.is_some() {
            format!(
                "SELECT CAST((julianday('now') - julianday(created_at)) * 86400 AS INTEGER)
                 FROM {} WHERE member_id = ? ORDER BY id DESC LIMIT 1",
                table
            )
        } else {
            format!(
                "SELECT CAST((julianday('now') - julianday(created_at)) * 86400 AS INTEGER)
                 FROM {} WHERE {} = ? ORDER BY id DESC LIMIT 1",
                table, ip_col
            )
        };

        let elapsed: Option<i64> = if let Some(mid) = member_id {
            sqlx::query_scalar(&sql).bind(mid).fetch_optional(&state.db).await?
        } else {
            sqlx::query_scalar(&sql).bind(ip).fetch_optional(&state.db).await?
        };

        if let Some(sec) = elapsed {
            if sec < settings.delay_sec {
                let wait = settings.delay_sec - sec;
                return Err(AppError::Forbidden(format!(
                    "도배 방지를 위해 {}초 후에 다시 작성할 수 있습니다.",
                    wait
                )));
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_post(
    state: &AppState,
    board: &Board,
    member: Option<&Member>,
    wr_name: &str,
    wr_password: Option<&str>,
    wr_category: Option<&str>,
    wr_subject: &str,
    raw_content: &str,
    is_notice: bool,
    is_secret: bool,
    wr_link1: Option<&str>,
    wr_link2: Option<&str>,
    ip: &str,
) -> Result<i64, AppError> {
    let user_level = member.map(|m| m.mb_level).unwrap_or(1);
    if user_level < board.bo_write_level {
        return Err(AppError::Forbidden(format!(
            "글쓰기 권한이 없습니다. (필요 레벨: {})",
            board.bo_write_level
        )));
    }

    let subject = wr_subject.trim();
    if subject.is_empty() {
        return Err(AppError::BadRequest("제목을 입력해주세요.".into()));
    }

    let clean_html = sanitize_editor_html(raw_content);
    if clean_html.trim().is_empty() {
        return Err(AppError::BadRequest("내용을 입력해주세요.".into()));
    }

    check_write_policy(
        state,
        user_level,
        ip,
        member.map(|m| m.id),
        &format!("{} {}", subject, clean_html),
        true,
        "posts",
    )
    .await?;

    let notice_flag = if is_notice && user_level >= 10 { 1 } else { 0 };
    let secret_flag = if is_secret { 1 } else { 0 };
    let author_name = member
        .map(|m| m.mb_name.as_str())
        .unwrap_or_else(|| wr_name.trim());
    if author_name.is_empty() {
        return Err(AppError::BadRequest("작성자 이름을 입력해주세요.".into()));
    }

    let pw_hash = if member.is_none() {
        let raw_pw = wr_password.map(|s| s.trim()).unwrap_or("");
        if raw_pw.len() < 4 {
            return Err(AppError::BadRequest(
                "비회원 글 작성 시 수정/삭제용 비밀번호를 4자 이상 입력해주세요.".into(),
            ));
        }
        Some(auth_service::hash_password(raw_pw)?)
    } else {
        None
    };

    let res = sqlx::query(
        "INSERT INTO posts
         (bo_table, member_id, wr_name, wr_password, wr_category, wr_subject, wr_content, is_notice, is_secret, wr_link1, wr_link2, wr_ip)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&board.bo_table)
    .bind(member.map(|m| m.id))
    .bind(author_name)
    .bind(pw_hash)
    .bind(wr_category.filter(|s| !s.trim().is_empty()))
    .bind(subject)
    .bind(&clean_html)
    .bind(notice_flag)
    .bind(secret_flag)
    .bind(wr_link1.filter(|s| !s.trim().is_empty()))
    .bind(wr_link2.filter(|s| !s.trim().is_empty()))
    .bind(ip)
    .execute(&state.db)
    .await?;

    let post_id = res.last_insert_rowid();

    file_service::sync_post_files(state, post_id, member.map(|m| m.id), &clean_html).await?;

    if let Some(m) = member {
        if board.bo_write_point != 0 && point_service::is_point_enabled(&state.db).await {
            let rel_id = format!("{}:{}", board.bo_table, post_id);
            let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
            let msg = crate::i18n::tr_sys(
                &sys_lang,
                "sys_msg.write_point",
                &[("board", &board.bo_subject), ("id", &post_id.to_string())],
            );
            let _ = point_service::apply_point(
                &state.db,
                &state.events,
                m.id,
                board.bo_write_point,
                Some("board"),
                Some(&rel_id),
                "write",
                &msg,
            )
            .await;
        }
        let _ = level_service::evaluate_auto_level_up(&state.db, &state.events, m.id).await;
    }

    state.events.publish(DomainEvent::PostCreated {
        post_id,
        bo_table: board.bo_table.clone(),
        member_id: member.map(|m| m.id),
    });

    Ok(post_id)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_comment(
    state: &AppState,
    board: &Board,
    post: &Post,
    member: Option<&Member>,
    co_name: &str,
    co_password: Option<&str>,
    co_content: &str,
    parent_id: Option<i64>,
    is_secret: bool,
    ip: &str,
) -> Result<i64, AppError> {
    let user_level = member.map(|m| m.mb_level).unwrap_or(1);
    if user_level < board.bo_comment_level {
        return Err(AppError::Forbidden(format!(
            "댓글 작성 권한이 없습니다. (필요 레벨: {})",
            board.bo_comment_level
        )));
    }

    let content = co_content.trim().to_string();
    if content.is_empty() {
        return Err(AppError::BadRequest("댓글 내용을 입력해주세요.".into()));
    }

    check_write_policy(
        state,
        user_level,
        ip,
        member.map(|m| m.id),
        &content,
        true,
        "comments",
    )
    .await?;

    let author_name = member
        .map(|m| m.mb_name.as_str())
        .unwrap_or_else(|| co_name.trim());
    if author_name.is_empty() {
        return Err(AppError::BadRequest("작성자 이름을 입력해주세요.".into()));
    }

    // Validate parent comment if replying
    let mut parent_owner_id: Option<i64> = None;
    if let Some(pid) = parent_id {
        let parent_row: Option<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT post_id, member_id FROM comments WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(pid)
        .fetch_optional(&state.db)
        .await?;
        let Some((p_post_id, p_member_id)) = parent_row else {
            return Err(AppError::NotFound("답글을 달 원댓글을 찾을 수 없습니다.".into()));
        };
        if p_post_id != post.id {
            return Err(AppError::BadRequest("잘못된 부모 댓글입니다.".into()));
        }
        parent_owner_id = p_member_id;
    }

    let pw_hash = if member.is_none() {
        let raw_pw = co_password.map(|s| s.trim()).unwrap_or("");
        if raw_pw.is_empty() {
            None
        } else {
            Some(auth_service::hash_password(raw_pw)?)
        }
    } else {
        None
    };

    let res = sqlx::query(
        "INSERT INTO comments (post_id, parent_id, member_id, co_name, co_password, co_content, co_ip, is_secret)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(post.id)
    .bind(parent_id)
    .bind(member.map(|m| m.id))
    .bind(author_name)
    .bind(pw_hash)
    .bind(&content)
    .bind(ip)
    .bind(if is_secret { 1 } else { 0 })
    .execute(&state.db)
    .await?;

    let comment_id = res.last_insert_rowid();

    sqlx::query("UPDATE posts SET wr_comment_count = wr_comment_count + 1 WHERE id = ?")
        .bind(post.id)
        .execute(&state.db)
        .await?;

    if let Some(m) = member {
        if board.bo_comment_point != 0 && point_service::is_point_enabled(&state.db).await {
            let rel_id = format!("{}:{}", board.bo_table, comment_id);
            let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
            let msg = crate::i18n::tr_sys(
                &sys_lang,
                "sys_msg.comment_point",
                &[("board", &board.bo_subject), ("id", &post.id.to_string())],
            );
            let _ = point_service::apply_point(
                &state.db,
                &state.events,
                m.id,
                board.bo_comment_point,
                Some("comment"),
                Some(&rel_id),
                "comment",
                &msg,
            )
            .await;
        }
        let _ = level_service::evaluate_auto_level_up(&state.db, &state.events, m.id).await;
    }

    let target_url = format!("/board/{}/{}#comment-{}", board.bo_table, post.id, comment_id);

    // Notify parent comment author on reply
    if let Some(p_owner) = parent_owner_id {
        if member.map(|m| m.id) != Some(p_owner) {
            let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
            let msg = crate::i18n::tr_sys(
                &sys_lang,
                "sys_msg.new_reply_noti",
                &[("subject", &post.wr_subject)],
            );
            let _ = notification_service::dispatch_notification(
                &state.db,
                &state.events,
                p_owner,
                "reply",
                &msg,
                Some(&target_url),
            )
            .await;
        }
    }

    // Notify post author if someone else commented (avoid duplicate notice if post author == parent comment author)
    if let Some(post_owner_id) = post.member_id {
        if member.map(|m| m.id) != Some(post_owner_id) && parent_owner_id != Some(post_owner_id) {
            let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
            let msg = crate::i18n::tr_sys(
                &sys_lang,
                "sys_msg.new_comment_noti",
                &[("subject", &post.wr_subject)],
            );
            let _ = notification_service::dispatch_notification(
                &state.db,
                &state.events,
                post_owner_id,
                "comment",
                &msg,
                Some(&target_url),
            )
            .await;
        }
    }

    state.events.publish(DomainEvent::CommentCreated {
        comment_id,
        post_id: post.id,
        member_id: member.map(|m| m.id),
    });

    Ok(comment_id)
}

fn verify_post_manage_permission(
    post: &Post,
    member: Option<&Member>,
    guest_password: Option<&str>,
) -> Result<(), AppError> {
    if let Some(m) = member {
        if m.mb_level >= 10 || post.member_id == Some(m.id) {
            return Ok(());
        }
        return Err(AppError::Forbidden(
            "본인 작성글 또는 관리자만 수정/삭제할 수 있습니다.".into(),
        ));
    }

    if post.member_id.is_some() {
        return Err(AppError::Forbidden(
            "회원 작성글은 작성자 본인 또는 관리자만 수정/삭제할 수 있습니다.".into(),
        ));
    }

    let pw_hash = post.wr_password.as_deref().ok_or_else(|| {
        AppError::Forbidden("비밀번호가 설정되지 않은 글은 관리자만 수정/삭제할 수 있습니다.".into())
    })?;
    let input_pw = guest_password.map(|s| s.trim()).unwrap_or("");
    if input_pw.is_empty() || !auth_service::verify_password(input_pw, pw_hash) {
        return Err(AppError::Forbidden("비밀번호가 일치하지 않습니다.".into()));
    }

    Ok(())
}

pub async fn get_post_for_edit(
    state: &AppState,
    board: &Board,
    post_id: i64,
    member: Option<&Member>,
) -> Result<Post, AppError> {
    let post = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE id = ? AND bo_table = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(&board.bo_table)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("게시글을 찾을 수 없습니다.".into()))?;

    if let Some(m) = member {
        if m.mb_level < 10 && post.member_id != Some(m.id) {
            return Err(AppError::Forbidden(
                "본인 작성글 또는 관리자만 수정할 수 있습니다.".into(),
            ));
        }
    } else {
        if post.member_id.is_some() {
            return Err(AppError::Forbidden(
                "회원 작성글은 로그인 후 작성자 본인 또는 관리자만 수정할 수 있습니다.".into(),
            ));
        }
        if post.is_secret != 0 {
            return Err(AppError::Forbidden(
                "비밀글은 작성자와 관리자만 열람/수정할 수 있습니다.".into(),
            ));
        }
    }

    Ok(post)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_post(
    state: &AppState,
    board: &Board,
    post_id: i64,
    member: Option<&Member>,
    guest_password: Option<&str>,
    wr_category: Option<&str>,
    wr_subject: &str,
    raw_content: &str,
    is_notice: bool,
    is_secret: bool,
    wr_link1: Option<&str>,
    wr_link2: Option<&str>,
) -> Result<(), AppError> {
    let post = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE id = ? AND bo_table = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(&board.bo_table)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("게시글을 찾을 수 없습니다.".into()))?;

    verify_post_manage_permission(&post, member, guest_password)?;

    let subject = wr_subject.trim();
    if subject.is_empty() {
        return Err(AppError::BadRequest("제목을 입력해주세요.".into()));
    }

    let clean_html = sanitize_editor_html(raw_content);
    if clean_html.trim().is_empty() {
        return Err(AppError::BadRequest("내용을 입력해주세요.".into()));
    }

    let user_level = member.map(|m| m.mb_level).unwrap_or(1);
    check_write_policy(
        state,
        user_level,
        "",
        member.map(|m| m.id),
        &format!("{} {}", subject, clean_html),
        false,
        "posts",
    )
    .await?;

    let notice_flag = if user_level >= 10 {
        if is_notice { 1 } else { 0 }
    } else {
        post.is_notice
    };
    let secret_flag = if is_secret { 1 } else { 0 };

    sqlx::query(
        "UPDATE posts
         SET wr_category = ?, wr_subject = ?, wr_content = ?, is_notice = ?, is_secret = ?,
             wr_link1 = ?, wr_link2 = ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?",
    )
    .bind(wr_category.filter(|s| !s.trim().is_empty()))
    .bind(subject)
    .bind(&clean_html)
    .bind(notice_flag)
    .bind(secret_flag)
    .bind(wr_link1.filter(|s| !s.trim().is_empty()))
    .bind(wr_link2.filter(|s| !s.trim().is_empty()))
    .bind(post.id)
    .execute(&state.db)
    .await?;

    file_service::sync_post_files(state, post.id, post.member_id, &clean_html).await?;

    state.events.publish(DomainEvent::PostUpdated {
        post_id: post.id,
        bo_table: board.bo_table.clone(),
    });

    Ok(())
}

pub async fn soft_delete_post(
    state: &AppState,
    board: &Board,
    post_id: i64,
    member: Option<&Member>,
    guest_password: Option<&str>,
) -> Result<(), AppError> {
    let post = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE id = ? AND bo_table = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(&board.bo_table)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("게시글을 찾을 수 없습니다.".into()))?;

    verify_post_manage_permission(&post, member, guest_password)?;

    sqlx::query("UPDATE posts SET deleted_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(post.id)
        .execute(&state.db)
        .await?;

    state.events.publish(DomainEvent::PostDeleted {
        post_id: post.id,
        bo_table: board.bo_table.clone(),
    });

    Ok(())
}

/// Build tree-ordered CommentView items with secret-comment masking and permission flags
pub fn build_comment_views(
    comments: Vec<Comment>,
    post: &Post,
    member: Option<&Member>,
) -> Vec<CommentView> {
    let mut by_parent: HashMap<Option<i64>, Vec<Comment>> = HashMap::new();
    let existing_ids: HashSet<i64> = comments.iter().map(|c| c.id).collect();
    for c in comments {
        let p = c.parent_id.filter(|pid| existing_ids.contains(pid));
        by_parent.entry(p).or_default().push(c);
    }

    let mut ordered = Vec::new();
    fn visit(
        parent: Option<i64>,
        depth: i64,
        by_parent: &mut HashMap<Option<i64>, Vec<Comment>>,
        post: &Post,
        member: Option<&Member>,
        out: &mut Vec<CommentView>,
    ) {
        if let Some(children) = by_parent.remove(&parent) {
            for mut c in children {
                let cid = c.id;
                let is_admin = member.map(|m| m.mb_level >= 10).unwrap_or(false);
                let is_post_owner = member.and_then(|m| post.member_id.map(|oid| oid == m.id)).unwrap_or(false);
                let is_comment_owner = member.and_then(|m| c.member_id.map(|cid_m| cid_m == m.id)).unwrap_or(false);

                let can_view_secret = c.is_secret == 0 || is_admin || is_post_owner || is_comment_owner;
                if !can_view_secret {
                    c.co_content = String::new();
                }

                let is_guest_comment = c.member_id.is_none();
                let can_manage = is_admin
                    || is_comment_owner
                    || (member.is_none() && is_guest_comment && c.co_password.is_some());

                out.push(CommentView {
                    comment: c,
                    depth: depth.min(4),
                    can_view_secret,
                    can_manage,
                    is_guest_comment,
                });

                visit(Some(cid), depth + 1, by_parent, post, member, out);
            }
        }
    }

    visit(None, 0, &mut by_parent, post, member, &mut ordered);
    ordered
}

fn verify_comment_manage_permission(
    comment: &Comment,
    member: Option<&Member>,
    guest_password: Option<&str>,
) -> Result<(), AppError> {
    if let Some(m) = member {
        if m.mb_level >= 10 || comment.member_id == Some(m.id) {
            return Ok(());
        }
        return Err(AppError::Forbidden(
            "본인 댓글 또는 관리자만 수정/삭제할 수 있습니다.".into(),
        ));
    }

    if comment.member_id.is_some() {
        return Err(AppError::Forbidden(
            "회원 댓글은 작성자 본인 또는 관리자만 수정/삭제할 수 있습니다.".into(),
        ));
    }

    let pw_hash = comment.co_password.as_deref().ok_or_else(|| {
        AppError::Forbidden("비밀번호가 설정되지 않은 댓글은 관리자만 수정/삭제할 수 있습니다.".into())
    })?;
    let input_pw = guest_password.map(|s| s.trim()).unwrap_or("");
    if input_pw.is_empty() || !auth_service::verify_password(input_pw, pw_hash) {
        return Err(AppError::Forbidden("댓글 비밀번호가 일치하지 않습니다.".into()));
    }
    Ok(())
}

pub async fn update_comment(
    state: &AppState,
    board: &Board,
    post_id: i64,
    comment_id: i64,
    member: Option<&Member>,
    guest_password: Option<&str>,
    co_content: &str,
    is_secret: bool,
) -> Result<(), AppError> {
    let _ = board;
    let comment = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE id = ? AND post_id = ? AND deleted_at IS NULL",
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("댓글을 찾을 수 없습니다.".into()))?;

    verify_comment_manage_permission(&comment, member, guest_password)?;

    let content = co_content.trim().to_string();
    if content.is_empty() {
        return Err(AppError::BadRequest("댓글 내용을 입력해주세요.".into()));
    }

    let user_level = member.map(|m| m.mb_level).unwrap_or(1);
    check_write_policy(state, user_level, "", member.map(|m| m.id), &content, false, "comments").await?;

    sqlx::query("UPDATE comments SET co_content = ?, is_secret = ? WHERE id = ?")
        .bind(&content)
        .bind(if is_secret { 1 } else { 0 })
        .bind(comment_id)
        .execute(&state.db)
        .await?;

    Ok(())
}

pub async fn soft_delete_comment(
    state: &AppState,
    board: &Board,
    post_id: i64,
    comment_id: i64,
    member: Option<&Member>,
    guest_password: Option<&str>,
) -> Result<(), AppError> {
    let _ = board;
    let comment = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE id = ? AND post_id = ? AND deleted_at IS NULL",
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("댓글을 찾을 수 없습니다.".into()))?;

    verify_comment_manage_permission(&comment, member, guest_password)?;

    sqlx::query(
        "UPDATE comments SET deleted_at = CURRENT_TIMESTAMP
         WHERE post_id = ? AND (id = ? OR parent_id = ?) AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(comment_id)
    .bind(comment_id)
    .execute(&state.db)
    .await?;

    let (cnt,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM comments WHERE post_id = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query("UPDATE posts SET wr_comment_count = ? WHERE id = ?")
        .bind(cnt)
        .bind(post_id)
        .execute(&state.db)
        .await?;

    Ok(())
}

/// Accept a Q&A answer via posts.extra_data JSON (zero schema change) and reward the answerer
pub async fn accept_qna_answer(
    state: &AppState,
    board: &Board,
    post_id: i64,
    comment_id: i64,
    member: Option<&Member>,
) -> Result<(), AppError> {
    let actor = member.ok_or_else(|| AppError::Unauthorized("로그인이 필요합니다.".into()))?;
    let post = sqlx::query_as::<_, Post>(
        "SELECT * FROM posts WHERE id = ? AND bo_table = ? AND deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(&board.bo_table)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("게시글을 찾을 수 없습니다.".into()))?;

    if actor.mb_level < 10 && post.member_id != Some(actor.id) {
        return Err(AppError::Forbidden("질문 작성자 또는 관리자만 답변을 채택할 수 있습니다.".into()));
    }

    let comment = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE id = ? AND post_id = ? AND deleted_at IS NULL",
    )
    .bind(comment_id)
    .bind(post.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("채택할 댓글을 찾을 수 없습니다.".into()))?;

    let mut extra: serde_json::Value = serde_json::from_str(&post.extra_data).unwrap_or_else(|_| serde_json::json!({}));
    if !extra.is_object() {
        extra = serde_json::json!({});
    }
    if extra.get("qna_solved").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(AppError::BadRequest("이미 채택된 답변이 존재합니다.".into()));
    }

    if let Some(obj) = extra.as_object_mut() {
        obj.insert("qna_solved".to_string(), serde_json::json!(true));
        obj.insert("accepted_comment_id".to_string(), serde_json::json!(comment.id));
        obj.insert("accepted_by".to_string(), serde_json::json!(actor.mb_id));
    }
    let extra_str = serde_json::to_string(&extra).unwrap_or_else(|_| "{}".to_string());

    sqlx::query("UPDATE posts SET extra_data = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&extra_str)
        .bind(post.id)
        .execute(&state.db)
        .await?;

    if let Some(answer_uid) = comment.member_id {
        if Some(answer_uid) != post.member_id {
            if point_service::is_point_enabled(&state.db).await {
                let reward = (board.bo_comment_point.max(2) * 5).max(20);
                let rel_id = format!("qna:{}:{}", board.bo_table, post.id);
                let already = point_service::has_point_log(
                    &state.db,
                    answer_uid,
                    "qna_accept",
                    &rel_id,
                    "qna_accept",
                )
                .await
                .unwrap_or(false);
                if !already {
                    let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
                    let pt_msg = crate::i18n::tr_sys(
                        &sys_lang,
                        "sys_msg.qna_accepted_point",
                        &[("board", &board.bo_subject), ("id", &post.id.to_string())],
                    );
                    let _ = point_service::apply_point(
                        &state.db,
                        &state.events,
                        answer_uid,
                        reward,
                        Some("qna_accept"),
                        Some(&rel_id),
                        "qna_accept",
                        &pt_msg,
                    )
                    .await;
                }
            }

            let sys_lang = crate::i18n::get_site_default_lang(&state.db).await;
            let noti_msg = crate::i18n::tr_sys(
                &sys_lang,
                "sys_msg.qna_accepted_noti",
                &[("subject", &post.wr_subject)],
            );
            let _ = notification_service::dispatch_notification(
                &state.db,
                &state.events,
                answer_uid,
                "qna_accept",
                &noti_msg,
                Some(&format!("/board/{}/{}#comment-{}", board.bo_table, post.id, comment.id)),
            )
            .await;
        }
    }

    Ok(())
}

/// Extract the first <img src="..."> URL from sanitized post HTML
pub fn extract_first_image(html: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#).ok()?;
    re.captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

/// Strip HTML tags into plain text summary for SEO/OpenGraph/RSS
pub fn strip_html_summary(html: &str, max_chars: usize) -> String {
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let plain = re.replace_all(html, " ");
    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(max_chars).collect();
    if collapsed.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

/// Build enriched PostListItem rows with thumbnail URL and parsed extra_data JSON for skins
pub async fn build_post_list_items(
    pool: &sqlx::SqlitePool,
    posts: Vec<Post>,
) -> Vec<PostListItem> {
    let (thumbs, mut extras) = build_list_meta(pool, &posts).await;
    posts
        .into_iter()
        .map(|p| {
            let k = p.id.to_string();
            let thumb = thumbs.get(&k).cloned();
            let extra = extras.remove(&k).unwrap_or_else(|| serde_json::json!({}));
            PostListItem {
                post: p,
                thumb,
                extra,
            }
        })
        .collect()
}
pub async fn build_list_meta(
    pool: &sqlx::SqlitePool,
    posts: &[Post],
) -> (HashMap<String, String>, HashMap<String, serde_json::Value>) {
    let mut thumbs: HashMap<String, String> = HashMap::new();
    let mut extras: HashMap<String, serde_json::Value> = HashMap::new();

    for p in posts {
        let key = p.id.to_string();
        if let Some(img) = extract_first_image(&p.wr_content) {
            thumbs.insert(key.clone(), img);
        }
        let ex: serde_json::Value =
            serde_json::from_str(&p.extra_data).unwrap_or_else(|_| serde_json::json!({}));
        extras.insert(key, ex);
    }

    // Fallback: look up attached image files for posts without inline <img>
    let missing_ids: Vec<i64> = posts
        .iter()
        .filter(|p| !thumbs.contains_key(&p.id.to_string()))
        .map(|p| p.id)
        .collect();

    if !missing_ids.is_empty() {
        let placeholders = missing_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT post_id, file_uuid FROM post_files
             WHERE status = 'active' AND file_type = 'image' AND post_id IN ({})
             ORDER BY id ASC",
            placeholders
        );
        let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
        for id in missing_ids {
            q = q.bind(id);
        }
        if let Ok(rows) = q.fetch_all(pool).await {
            for (pid, uuid) in rows {
                thumbs
                    .entry(pid.to_string())
                    .or_insert_with(|| format!("/api/download/{}", uuid));
            }
        }
    }

    (thumbs, extras)
}
