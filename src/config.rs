use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub dev_mode: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

fn default_path_style() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct S3Config {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default = "default_path_style")]
    pub path_style: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub driver: String,
    pub upload_dir: String,
    pub max_file_size_mb: u64,
    #[serde(default)]
    pub s3: S3Config,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub session_cookie_name: String,
    pub session_ttl_hours: i64,
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
