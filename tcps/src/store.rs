use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub sha256: String,
    pub name: String,
    pub size: u64,
    pub created: u64,
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

struct Inner {
    files: PathBuf,
    manifest_path: PathBuf,
    manifest: Mutex<Vec<FileMeta>>,
}

impl Store {
    pub fn new(root: &Path, user_id: &str) -> io::Result<Self> {
        let root = root.join(user_id);
        let files = root.join("files");
        fs::create_dir_all(&files)?;

        let manifest_path = root.join("manifest.json");
        let manifest = load_manifest(&manifest_path).unwrap_or_else(|_| scan_manifest(&files));

        let store = Self {
            inner: Arc::new(Inner {
                files,
                manifest_path,
                manifest: Mutex::new(manifest),
            }),
        };
        store.save_manifest()?;
        Ok(store)
    }

    pub fn manifest(&self) -> Vec<FileMeta> {
        lock(&self.inner.manifest).clone()
    }

    pub fn stats(&self) -> (usize, u64) {
        let manifest = lock(&self.inner.manifest);
        (manifest.len(), manifest.iter().map(|file| file.size).sum())
    }

    pub async fn put(&self, sha: &str, name: &str, bytes: &[u8]) -> Result<FileMeta, String> {
        if !is_hex_64(sha) {
            return Err("sha256 must be 64 lowercase hex characters".to_owned());
        }
        if hex(&Sha256::digest(bytes)) != sha {
            return Err("sha256 does not match the body".to_owned());
        }

        let name = safe_name(name, sha);
        let temporary = self.inner.files.join(format!(".{sha}.tmp"));
        let destination = self.inner.files.join(sha);

        tokio::fs::write(&temporary, bytes)
            .await
            .map_err(|error| format!("write failed: {error}"))?;
        tokio::fs::rename(&temporary, &destination)
            .await
            .map_err(|error| format!("rename failed: {error}"))?;

        let meta = FileMeta {
            sha256: sha.to_owned(),
            name,
            size: bytes.len() as u64,
            created: now(),
        };

        let mut manifest = lock(&self.inner.manifest);
        manifest.retain(|file| file.sha256 != sha);
        manifest.push(meta.clone());
        manifest.sort_by(|left, right| left.name.cmp(&right.name));
        save_manifest(&self.inner.manifest_path, &manifest)
            .map_err(|error| format!("manifest write failed: {error}"))?;

        Ok(meta)
    }

    pub async fn read(&self, sha: &str) -> io::Result<Vec<u8>> {
        if !is_hex_64(sha) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid sha256",
            ));
        }
        tokio::fs::read(self.inner.files.join(sha)).await
    }

    pub async fn delete(&self, sha: &str) -> io::Result<bool> {
        if !is_hex_64(sha) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid sha256",
            ));
        }

        let path = self.inner.files.join(sha);
        let existed = match tokio::fs::remove_file(path).await {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };

        let mut manifest = lock(&self.inner.manifest);
        let before = manifest.len();
        manifest.retain(|file| file.sha256 != sha);
        if manifest.len() != before {
            save_manifest(&self.inner.manifest_path, &manifest)?;
        }

        Ok(existed)
    }

    fn save_manifest(&self) -> io::Result<()> {
        let manifest = lock(&self.inner.manifest);
        save_manifest(&self.inner.manifest_path, &manifest)
    }
}

fn load_manifest(path: &Path) -> io::Result<Vec<FileMeta>> {
    let source = fs::read_to_string(path)?;
    serde_json::from_str(&source).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn scan_manifest(files: &Path) -> Vec<FileMeta> {
    let mut manifest = Vec::new();

    let Ok(entries) = fs::read_dir(files) else {
        return manifest;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(sha) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_hex_64(sha) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        manifest.push(FileMeta {
            sha256: sha.to_owned(),
            name: sha.to_owned(),
            size: metadata.len(),
            created: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0),
        });
    }

    manifest
}

fn save_manifest(path: &Path, manifest: &[FileMeta]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    fs::write(&temporary, data)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn safe_name(name: &str, fallback: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("").trim();

    let cleaned: String = base
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '-' | '_' | ' ' | '(' | ')')
        })
        .take(128)
        .collect();

    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn is_hex_64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_and_removes_content_addressed_files() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path(), "user").unwrap();
        let bytes = b"hello tcps";
        let sha = hex(&Sha256::digest(bytes));

        let meta = store.put(&sha, "notes.txt", bytes).await.unwrap();
        assert_eq!(meta.name, "notes.txt");
        assert_eq!(store.manifest().len(), 1);
        assert_eq!(store.read(&sha).await.unwrap(), bytes);

        assert!(store.delete(&sha).await.unwrap());
        assert!(store.manifest().is_empty());
        assert!(store.read(&sha).await.is_err());
    }

    #[tokio::test]
    async fn rejects_a_bad_hash() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path(), "user").unwrap();
        let error = store
            .put(&"a".repeat(64), "bad.bin", b"body")
            .await
            .unwrap_err();
        assert!(error.contains("does not match"));
    }
}
