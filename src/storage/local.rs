use super::StorageBackend;
use async_trait::async_trait;
use bytes::Bytes;
use std::io;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn resolve_safe_path(&self, key: &str) -> io::Result<PathBuf> {
        let rel = Path::new(key);
        for comp in rel.components() {
            match comp {
                Component::Normal(_) => {}
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Invalid storage key path",
                    ))
                }
            }
        }
        Ok(self.base_dir.join(rel))
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn put(&self, key: &str, data: Bytes) -> io::Result<()> {
        let target = self.resolve_safe_path(key)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(target, data).await
    }

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.resolve_safe_path(key).ok()
    }

    async fn get(&self, key: &str) -> io::Result<Bytes> {
        let target = self.resolve_safe_path(key)?;
        let data = fs::read(target).await?;
        Ok(Bytes::from(data))
    }

    async fn delete(&self, key: &str) -> io::Result<()> {
        let target = self.resolve_safe_path(key)?;
        match fs::remove_file(target).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}
