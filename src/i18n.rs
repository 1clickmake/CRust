use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

static GLOBAL_TRANSLATIONS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();

pub const LANG_COOKIE_NAME: &str = "rust_board_lang";

#[derive(Debug, Clone, Serialize)]
pub struct LangOption {
    pub code: &'static str,
    pub label: &'static str,
    pub flag: &'static str,
}

pub const SUPPORTED_LANGS: &[LangOption] = &[
    LangOption { code: "ko", label: "한국어", flag: "🇰🇷" },
    LangOption { code: "en", label: "English", flag: "🇺🇸" },
    LangOption { code: "ja", label: "日本語", flag: "🇯🇵" },
    LangOption { code: "zh", label: "中文", flag: "🇨🇳" },
];

pub fn is_supported_lang(code: &str) -> bool {
    SUPPORTED_LANGS.iter().any(|l| l.code == code)
}

fn flatten_json(prefix: &str, val: &serde_json::Value, out: &mut HashMap<String, String>) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let next_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_json(&next_key, v, out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        serde_json::Value::Number(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        serde_json::Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        _ => {}
    }
}

pub fn load_translations() -> HashMap<String, HashMap<String, String>> {
    let mut all = HashMap::new();
    for lang in SUPPORTED_LANGS {
        let path = Path::new("locales").join(format!("{}.json", lang.code));
        if let Ok(content) = fs::read_to_string(&path) {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    let mut map = HashMap::new();
                    flatten_json("", &val, &mut map);
                    all.insert(lang.code.to_string(), map);
                }
                Err(e) => {
                    tracing::warn!("[i18n] Failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }
    all
}

pub fn translate(
    translations: &HashMap<String, HashMap<String, String>>,
    lang: &str,
    key: &str,
) -> String {
    if let Some(map) = translations.get(lang) {
        if let Some(val) = map.get(key) {
            return val.clone();
        }
    }
    if lang != "ko" {
        if let Some(ko_map) = translations.get("ko") {
            if let Some(val) = ko_map.get(key) {
                return val.clone();
            }
        }
    }
    key.to_string()
}

pub fn resolve_lang(
    cookie_lang: Option<&str>,
    site_default_lang: &str,
    accept_language: Option<&str>,
) -> String {
    if let Some(c) = cookie_lang {
        if is_supported_lang(c) {
            return c.to_string();
        }
    }

    if site_default_lang != "auto" && is_supported_lang(site_default_lang) {
        return site_default_lang.to_string();
    }

    if let Some(hdr) = accept_language {
        for part in hdr.split(',') {
            let tag = part.split(';').next().unwrap_or("").trim().to_lowercase();
            let prefix = tag.split('-').next().unwrap_or("");
            if is_supported_lang(prefix) {
                return prefix.to_string();
            }
        }
    }

    "ko".to_string()
}

pub fn get_global_translations() -> &'static HashMap<String, HashMap<String, String>> {
    GLOBAL_TRANSLATIONS.get_or_init(load_translations)
}

pub async fn get_site_default_lang(pool: &sqlx::SqlitePool) -> String {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT cf_value FROM site_configs WHERE cf_key = 'default_lang'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    if let Some((val,)) = row {
        if is_supported_lang(&val) {
            return val;
        }
    }
    "ko".to_string()
}

pub fn tr_sys(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let mut msg = translate(get_global_translations(), lang, key);
    for (k, v) in vars {
        msg = msg.replace(&format!("{{{}}}", k), v);
    }
    msg
}
