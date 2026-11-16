use crate::config::AppConfig;
use crate::events::EventBus;
use crate::i18n;
use crate::storage::StorageBackend;
use minijinja::Environment;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub active_theme: Arc<RwLock<String>>,
    pub storage: Arc<dyn StorageBackend>,
    pub events: Arc<EventBus>,
    pub jinja: Arc<Environment<'static>>,
    pub i18n: Arc<HashMap<String, HashMap<String, String>>>,
}

impl AppState {
    pub fn build_jinja_env(
        i18n_data: Arc<HashMap<String, HashMap<String, String>>>,
    ) -> Environment<'static> {
        let mut env = Environment::new();
        env.set_loader(minijinja::path_loader("templates"));

        // Register {{ theme_asset(current_theme, "css/style.css") }} helper with fallback to default
        env.add_function("theme_asset", |theme: &str, asset_path: &str| -> String {
            let clean_theme = if is_safe_slug(theme) { theme } else { "default" };
            let clean_path = asset_path.trim_start_matches('/');
            if !clean_path.contains("..") {
                let candidate = Path::new("templates/themes")
                    .join(clean_theme)
                    .join("assets")
                    .join(clean_path);
                if candidate.exists() {
                    return format!("/theme-assets/{}/{}", clean_theme, clean_path);
                }
            }
            format!("/theme-assets/default/{}", clean_path)
        });

        // Register {{ t("common.login") }} function and {{ "common.login" | t }} filter
        let i18n_fn = i18n_data.clone();
        env.add_function("t", move |state: &minijinja::State, key: &str| -> String {
            let lang = state
                .lookup("current_lang")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "ko".to_string());
            i18n::translate(&i18n_fn, &lang, key)
        });

        let i18n_flt = i18n_data;
        env.add_filter("t", move |state: &minijinja::State, key: &str| -> String {
            let lang = state
                .lookup("current_lang")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "ko".to_string());
            i18n::translate(&i18n_flt, &lang, key)
        });

        env.add_filter("comma", |val: i64| -> String {
            let neg = val < 0;
            let digits = val.unsigned_abs().to_string();
            let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
            if neg {
                out.push('-');
            }
            for (i, ch) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(ch);
            }
            out
        });

        env
    }

    pub fn render_template<S: serde::Serialize>(
        &self,
        template_name: &str,
        ctx: S,
    ) -> Result<String, minijinja::Error> {
        if self.config.server.dev_mode {
            let fresh_i18n = Arc::new(i18n::load_translations());
            let env = Self::build_jinja_env(fresh_i18n);
            let tmpl = env.get_template(template_name)?;
            return tmpl.render(ctx);
        }
        let tmpl = self.jinja.get_template(template_name)?;
        tmpl.render(ctx)
    }
}

pub fn is_safe_slug(val: &str) -> bool {
    !val.is_empty()
        && val.len() <= 64
        && val
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
