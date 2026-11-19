use crate::error::AppError;
use crate::services::settings_service;
use sqlx::SqlitePool;

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn build_robots_txt(base_url: &str) -> String {
    format!(
        "User-agent: *\nAllow: /\nDisallow: /admin\nDisallow: /api/\nDisallow: /member/login\nDisallow: /member/register\nDisallow: /member/profile\nDisallow: /member/memo\n\nSitemap: {}/sitemap.xml\n",
        base_url.trim_end_matches('/')
    )
}

#[derive(sqlx::FromRow)]
struct SitemapPostRow {
    id: i64,
    bo_table: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct RssPostRow {
    id: i64,
    bo_table: String,
    wr_subject: String,
    wr_name: String,
    wr_content: String,
    created_at: String,
}

pub async fn build_sitemap_xml(pool: &SqlitePool, base_url: &str) -> Result<String, AppError> {
    let base = base_url.trim_end_matches('/');
    let boards: Vec<(String,)> = sqlx::query_as(
        "SELECT bo_table FROM boards WHERE bo_list_level <= 1 AND bo_read_level <= 1 ORDER BY bo_table ASC",
    )
    .fetch_all(pool)
    .await?;

    let posts = sqlx::query_as::<_, SitemapPostRow>(
        "SELECT p.id, p.bo_table, p.updated_at
         FROM posts p
         INNER JOIN boards b ON b.bo_table = p.bo_table
         WHERE p.deleted_at IS NULL AND p.is_secret = 0 AND b.bo_read_level <= 1
         ORDER BY p.id DESC LIMIT 1000",
    )
    .fetch_all(pool)
    .await?;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    xml.push_str(&format!(
        "  <url><loc>{}/</loc><changefreq>daily</changefreq><priority>1.0</priority></url>\n",
        escape_xml(base)
    ));

    for (bo_table,) in boards {
        xml.push_str(&format!(
            "  <url><loc>{}/board/{}</loc><changefreq>hourly</changefreq><priority>0.8</priority></url>\n",
            escape_xml(base),
            escape_xml(&bo_table)
        ));
    }

    for p in posts {
        let lastmod = p.updated_at.get(..10).unwrap_or(&p.updated_at);
        xml.push_str(&format!(
            "  <url><loc>{}/board/{}/{}</loc><lastmod>{}</lastmod><changefreq>weekly</changefreq><priority>0.6</priority></url>\n",
            escape_xml(base),
            escape_xml(&p.bo_table),
            p.id,
            escape_xml(lastmod)
        ));
    }

    let custom_pages: Vec<(String, String)> = sqlx::query_as(
        "SELECT slug, updated_at FROM custom_pages WHERE is_published = 1 ORDER BY slug ASC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (slug, updated_at) in custom_pages {
        let lastmod = updated_at.get(..10).unwrap_or(&updated_at);
        xml.push_str(&format!(
            "  <url><loc>{}/page/{}</loc><lastmod>{}</lastmod><changefreq>weekly</changefreq><priority>0.7</priority></url>\n",
            escape_xml(base),
            escape_xml(&slug),
            escape_xml(lastmod)
        ));
    }

    xml.push_str("</urlset>\n");
    Ok(xml)
}

pub async fn build_rss_xml(pool: &SqlitePool, base_url: &str) -> Result<String, AppError> {
    let base = base_url.trim_end_matches('/');
    let settings = settings_service::load_all(pool).await;
    let posts = sqlx::query_as::<_, RssPostRow>(
        "SELECT p.id, p.bo_table, p.wr_subject, p.wr_name, p.wr_content, p.created_at
         FROM posts p
         INNER JOIN boards b ON b.bo_table = p.bo_table
         WHERE p.deleted_at IS NULL AND p.is_secret = 0 AND b.bo_read_level <= 1
         ORDER BY p.id DESC LIMIT 30",
    )
    .fetch_all(pool)
    .await?;

    let channel_desc = if settings.site_description.trim().is_empty() {
        format!("{} Latest Posts RSS Feed", settings.site_title)
    } else {
        settings.site_description.clone()
    };

    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">\n<channel>\n  <title>{}</title>\n  <link>{}/</link>\n  <description>{}</description>\n",
        escape_xml(&settings.site_title),
        escape_xml(base),
        escape_xml(&channel_desc)
    );

    for p in posts {
        let link = format!("{}/board/{}/{}", base, p.bo_table, p.id);
        let desc = crate::services::board_service::strip_html_summary(&p.wr_content, 240);
        xml.push_str(&format!(
            "  <item>\n    <title>{}</title>\n    <link>{}</link>\n    <guid>{}</guid>\n    <author>{}</author>\n    <pubDate>{}</pubDate>\n    <description>{}</description>\n  </item>\n",
            escape_xml(&p.wr_subject),
            escape_xml(&link),
            escape_xml(&link),
            escape_xml(&p.wr_name),
            escape_xml(&p.created_at),
            escape_xml(&desc)
        ));
    }

    xml.push_str("</channel>\n</rss>\n");
    Ok(xml)
}

fn serialize_json_ld(val: &serde_json::Value) -> String {
    serde_json::to_string(val)
        .unwrap_or_default()
        .replace("</", "<\\/")
}

pub fn build_website_json_ld(base_url: &str, site_title: &str, description: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let val = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "WebSite",
        "name": site_title,
        "url": format!("{}/", base),
        "description": description,
        "potentialAction": {
            "@type": "SearchAction",
            "target": format!("{}/board/free?q={{search_term_string}}", base),
            "query-input": "required name=search_term_string"
        }
    });
    serialize_json_ld(&val)
}

pub fn build_board_json_ld(
    base_url: &str,
    bo_table: &str,
    bo_subject: &str,
    canonical_url: &str,
) -> String {
    let base = base_url.trim_end_matches('/');
    let val = serde_json::json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "CollectionPage",
                "name": bo_subject,
                "url": canonical_url
            },
            {
                "@type": "BreadcrumbList",
                "itemListElement": [
                    {
                        "@type": "ListItem",
                        "position": 1,
                        "name": "Home",
                        "item": format!("{}/", base)
                    },
                    {
                        "@type": "ListItem",
                        "position": 2,
                        "name": bo_subject,
                        "item": format!("{}/board/{}", base, bo_table)
                    }
                ]
            }
        ]
    });
    serialize_json_ld(&val)
}

#[allow(clippy::too_many_arguments)]
pub fn build_post_json_ld(
    base_url: &str,
    bo_table: &str,
    bo_subject: &str,
    post_id: i64,
    subject: &str,
    summary: &str,
    author: &str,
    created_at: &str,
    updated_at: &str,
    comment_count: usize,
    image_url: Option<&str>,
) -> String {
    let base = base_url.trim_end_matches('/');
    let post_url = format!("{}/board/{}/{}", base, bo_table, post_id);
    let mut posting = serde_json::json!({
        "@type": "DiscussionForumPosting",
        "headline": subject,
        "text": summary,
        "url": post_url,
        "datePublished": created_at,
        "dateModified": updated_at,
        "author": {
            "@type": "Person",
            "name": author
        },
        "interactionStatistic": {
            "@type": "InteractionCounter",
            "interactionType": "https://schema.org/CommentAction",
            "userInteractionCount": comment_count
        }
    });
    if let Some(img) = image_url.filter(|s| !s.is_empty()) {
        posting["image"] = serde_json::json!(img);
    }

    let val = serde_json::json!({
        "@context": "https://schema.org",
        "@graph": [
            posting,
            {
                "@type": "BreadcrumbList",
                "itemListElement": [
                    {
                        "@type": "ListItem",
                        "position": 1,
                        "name": "Home",
                        "item": format!("{}/", base)
                    },
                    {
                        "@type": "ListItem",
                        "position": 2,
                        "name": bo_subject,
                        "item": format!("{}/board/{}", base, bo_table)
                    },
                    {
                        "@type": "ListItem",
                        "position": 3,
                        "name": subject,
                        "item": post_url
                    }
                ]
            }
        ]
    });
    serialize_json_ld(&val)
}
