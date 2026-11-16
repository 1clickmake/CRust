pub mod local;
pub mod s3;

use crate::config::StorageConfig;
use async_trait::async_trait;
use bytes::Bytes;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Save bytes at a safe relative key (e.g. "editor/2611/uuid.png")
    async fn put(&self, key: &str, data: Bytes) -> io::Result<()>;

    /// Resolve local file path if supported (for fast streaming)
    fn local_path(&self, key: &str) -> Option<PathBuf>;

    /// Read full bytes
    async fn get(&self, key: &str) -> io::Result<Bytes>;

    /// Delete file by key
    async fn delete(&self, key: &str) -> io::Result<()>;
}

pub fn build_storage_backend(cfg: &StorageConfig) -> Arc<dyn StorageBackend> {
    if cfg.driver.eq_ignore_ascii_case("s3") {
        Arc::new(s3::S3Storage::new(&cfg.s3))
    } else {
        Arc::new(local::LocalStorage::new(&cfg.upload_dir))
    }
}
