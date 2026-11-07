use std::io;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

/// เก็บไฟล์สลิปใน SLIP_DIR: `orders/<id>.<ext>` และ `rejected/<id>.<ext>`
#[derive(Debug, Clone)]
pub struct SlipStore {
    dir: PathBuf,
}

impl SlipStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        SlipStore { dir: dir.into() }
    }

    async fn ensure(&self, sub: &str) -> io::Result<PathBuf> {
        let d = self.dir.join(sub);
        tokio::fs::create_dir_all(&d).await?;
        Ok(d)
    }

    pub async fn save_temp(&self, bytes: &[u8]) -> io::Result<PathBuf> {
        let path = self
            .ensure("tmp")
            .await?
            .join(format!("tmp-{}", Uuid::new_v4()));
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }

    async fn move_into(&self, temp: &Path, sub: &str, name: &str) -> io::Result<String> {
        let dest = self.ensure(sub).await?.join(name);
        tokio::fs::rename(temp, dest).await?;
        Ok(format!("{sub}/{name}"))
    }

    /// ย้ายไฟล์ temp ไปเป็นสลิปของคำสั่งซื้อ คืน relative path
    pub async fn finalize(&self, temp: &Path, name: &str) -> io::Result<String> {
        self.move_into(temp, "orders", name).await
    }

    /// ย้ายไฟล์ temp ไปเก็บเป็นหลักฐานความพยายามที่ถูกปฏิเสธ
    pub async fn reject_temp(&self, temp: &Path, name: &str) -> io::Result<String> {
        self.move_into(temp, "rejected", name).await
    }

    pub async fn save_rejected(&self, bytes: &[u8], name: &str) -> io::Result<String> {
        let path = self.ensure("rejected").await?.join(name);
        tokio::fs::write(path, bytes).await?;
        Ok(format!("rejected/{name}"))
    }

    pub async fn remove(&self, path: &Path) {
        let _ = tokio::fs::remove_file(path).await;
    }

    pub async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        let rel_path = Path::new(rel);
        if rel_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad path"));
        }
        tokio::fs::read(self.dir.join(rel_path)).await
    }
}
