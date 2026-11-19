use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteSettings {
    // 1. 홈페이지 기본 설정
    pub site_title: String,
    pub site_description: String,
    pub admin_email: String,
    pub default_lang: String,
    pub page_rows: i64,
    pub write_pages: i64,
    pub analytics_script: String,
    pub add_meta: String,

    // 2. 전역 포인트 정책
    pub use_point: bool,
    pub login_point: i64,
    pub register_point: i64,
    pub memo_send_point: i64,

    // 3. 게시판 기본 설정 & 스팸 방어
    pub delay_sec: i64,
    pub filter_words: String,
    pub link_target: String,

    // 4. 회원가입 설정
    pub allow_register: bool,
    pub register_level: i64,
    pub prohibit_id: String,
    pub stipulation: String,
    pub privacy: String,

    // 5. 접근 제어 & IP 보안
    pub possible_ip: String,
    pub intercept_ip: String,
    pub img_max_width: i64,
    pub img_webp_enabled: bool,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            site_title: "CRust".to_string(),
            site_description: "CRust — Rust(Axum + SQLite FTS5) 기반 고속 커뮤니티 포럼".to_string(),
            admin_email: "admin@example.com".to_string(),
            default_lang: "ko".to_string(),
            page_rows: 15,
            write_pages: 10,
            analytics_script: String::new(),
            add_meta: String::new(),
            use_point: true,
            login_point: 100,
            register_point: 1000,
            memo_send_point: 50,
            delay_sec: 30,
            filter_words: String::new(),
            link_target: "_blank".to_string(),
            allow_register: true,
            register_level: 2,
            prohibit_id: "admin,administrator,root,system,manager,guest".to_string(),
            stipulation: String::new(),
            privacy: String::new(),
            possible_ip: String::new(),
            intercept_ip: String::new(),
            img_max_width: 1920,
            img_webp_enabled: true,
        }
    }
}

pub async fn load_all(pool: &SqlitePool) -> SiteSettings {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT cf_key, cf_value FROM site_configs")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let map: HashMap<String, String> = rows.into_iter().collect();
    let d = SiteSettings::default();

    let get_str = |k: &str, def: String| -> String {
        map.get(k).cloned().unwrap_or(def)
    };
    let get_i64 = |k: &str, def: i64| -> i64 {
        map.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(def)
    };
    let get_bool = |k: &str, def: bool| -> bool {
        map.get(k).map(|v| v != "0" && v != "false").unwrap_or(def)
    };

    SiteSettings {
        site_title: get_str("site_title", d.site_title),
        site_description: get_str("site_description", d.site_description),
        admin_email: get_str("admin_email", d.admin_email),
        default_lang: {
            let v = get_str("default_lang", d.default_lang);
            if v == "auto" || crate::i18n::is_supported_lang(&v) {
                v
            } else {
                "ko".to_string()
            }
        },
        page_rows: get_i64("page_rows", d.page_rows).clamp(5, 100),
        write_pages: get_i64("write_pages", d.write_pages).clamp(3, 20),
        analytics_script: get_str("analytics_script", d.analytics_script),
        add_meta: get_str("add_meta", d.add_meta),
        use_point: get_bool("use_point", d.use_point),
        login_point: get_i64("login_point", d.login_point).max(0),
        register_point: get_i64("register_point", d.register_point).max(0),
        memo_send_point: get_i64("memo_send_point", d.memo_send_point).max(0),
        delay_sec: get_i64("delay_sec", d.delay_sec).max(0),
        filter_words: get_str("filter_words", d.filter_words),
        link_target: match get_str("link_target", d.link_target).as_str() {
            "_self" => "_self".to_string(),
            _ => "_blank".to_string(),
        },
        allow_register: get_bool("allow_register", d.allow_register),
        register_level: get_i64("register_level", d.register_level).clamp(1, 10),
        prohibit_id: get_str("prohibit_id", d.prohibit_id),
        stipulation: get_str("stipulation", d.stipulation),
        privacy: get_str("privacy", d.privacy),
        possible_ip: get_str("possible_ip", d.possible_ip),
        intercept_ip: get_str("intercept_ip", d.intercept_ip),
        img_max_width: get_i64("img_max_width", d.img_max_width).clamp(0, 4096),
        img_webp_enabled: get_bool("img_webp_enabled", d.img_webp_enabled),
    }
}

pub async fn save_all(pool: &SqlitePool, s: &SiteSettings) -> Result<(), AppError> {
    let valid_lang = if s.default_lang == "auto" || crate::i18n::is_supported_lang(&s.default_lang) {
        s.default_lang.clone()
    } else {
        "ko".to_string()
    };

    let pairs: [(&str, String); 24] = [
        ("site_title", s.site_title.trim().to_string()),
        ("site_description", s.site_description.trim().to_string()),
        ("admin_email", s.admin_email.trim().to_string()),
        ("default_lang", valid_lang),
        ("page_rows", s.page_rows.clamp(5, 100).to_string()),
        ("write_pages", s.write_pages.clamp(3, 20).to_string()),
        ("analytics_script", s.analytics_script.clone()),
        ("add_meta", s.add_meta.clone()),
        ("use_point", if s.use_point { "1" } else { "0" }.to_string()),
        ("login_point", s.login_point.max(0).to_string()),
        ("register_point", s.register_point.max(0).to_string()),
        ("memo_send_point", s.memo_send_point.max(0).to_string()),
        ("delay_sec", s.delay_sec.max(0).to_string()),
        ("filter_words", s.filter_words.trim().to_string()),
        ("link_target", if s.link_target == "_self" { "_self" } else { "_blank" }.to_string()),
        ("allow_register", if s.allow_register { "1" } else { "0" }.to_string()),
        ("register_level", s.register_level.clamp(1, 10).to_string()),
        ("prohibit_id", s.prohibit_id.trim().to_string()),
        ("stipulation", s.stipulation.clone()),
        ("privacy", s.privacy.clone()),
        ("possible_ip", s.possible_ip.trim().to_string()),
        ("intercept_ip", s.intercept_ip.trim().to_string()),
        ("img_max_width", s.img_max_width.clamp(0, 4096).to_string()),
        ("img_webp_enabled", if s.img_webp_enabled { "1" } else { "0" }.to_string()),
    ];

    let mut tx = pool.begin().await?;
    for (k, v) in pairs {
        sqlx::query(
            "INSERT INTO site_configs (cf_key, cf_value) VALUES (?, ?)
             ON CONFLICT(cf_key) DO UPDATE SET cf_value = excluded.cf_value",
        )
        .bind(k)
        .bind(v)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Check if text contains any prohibited filter word
pub fn find_filtered_word(filter_words: &str, text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for raw in filter_words.split(',') {
        let word = raw.trim();
        if !word.is_empty() && lower.contains(&word.to_lowercase()) {
            return Some(word.to_string());
        }
    }
    None
}

/// Check if client IP matches newline-separated IP list (exact or prefix like "192.168.")
pub fn ip_matches_list(list: &str, client_ip: &str) -> bool {
    let ip = client_ip.trim();
    for line in list.lines() {
        let pattern = line.trim();
        if pattern.is_empty() || pattern.starts_with('#') {
            continue;
        }
        if ip == pattern || ip.starts_with(pattern) {
            return true;
        }
    }
    false
}
