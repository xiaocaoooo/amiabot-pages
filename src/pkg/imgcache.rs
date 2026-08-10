use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use once_cell::sync::Lazy;
use crate::pkg::http_error::format_upstream_http_error;

const DEFAULT_CACHE_DIR: &str = "cache/images";
const DEFAULT_MAX_SIZE_MB: usize = 512;

#[derive(Clone, Debug)]
pub struct CacheMeta {
    pub created_at: SystemTime,
    pub ttl: Option<Duration>,
    pub size: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileCacheEntry {
    pub url: String,
    pub data_url: String,
    pub created_at: u64, // ms
    pub ttl_ms: i64,    // ms, -1 means no TTL
}

pub struct ImageCache {
    items: RwLock<HashMap<String, CacheMeta>>,
    total_size: RwLock<usize>,
    max_size: usize,
    cache_dir: String,
    client: reqwest::Client,
}

pub static DEFAULT_IMG_CACHE: Lazy<Arc<ImageCache>> = Lazy::new(|| {
    let max_mb = std::env::var("IMAGE_CACHE_MAX_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_SIZE_MB);
    
    Arc::new(ImageCache::new(max_mb * 1024 * 1024, DEFAULT_CACHE_DIR.to_string()))
});

impl ImageCache {
    pub fn new(max_size: usize, cache_dir: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            items: RwLock::new(HashMap::new()),
            total_size: RwLock::new(0),
            max_size,
            cache_dir,
            client,
        }
    }

    fn cache_key(&self, image_url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(image_url.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn file_path(&self, key: &str) -> PathBuf {
        Path::new(&self.cache_dir).join(format!("{}.json", key))
    }

    fn is_expired(&self, meta: &CacheMeta) -> bool {
        if let Some(ttl) = meta.ttl {
            if let Ok(elapsed) = meta.created_at.elapsed() {
                return elapsed > ttl;
            }
        }
        false
    }

    pub async fn load_index(&self) {
        let path = Path::new(&self.cache_dir);
        if !path.exists() {
            tracing::info!(
                cache_dir = %self.cache_dir,
                loaded = 0,
                size_mb = 0,
                "图片缓存索引已加载（目录不存在）"
            );
            return;
        }
        let mut items_write = self.items.write().await;
        let mut total_write = self.total_size.write().await;
        
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    cache_dir = %self.cache_dir,
                    error = %e,
                    "读取图片缓存目录失败"
                );
                return;
            }
        };

        let mut loaded = 0;
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_file() && file_path.extension().and_then(|s| s.to_str()) == Some("json") {
                let key = match file_path.file_stem().and_then(|s| s.to_str()) {
                    Some(k) => k.to_string(),
                    None => continue,
                };
                if let Ok(data) = fs::read_to_string(&file_path) {
                    if let Ok(entry) = serde_json::from_str::<FileCacheEntry>(&data) {
                        let ttl = if entry.ttl_ms >= 0 {
                            Some(Duration::from_millis(entry.ttl_ms as u64))
                        } else {
                            None
                        };
                        let created_at = SystemTime::UNIX_EPOCH + Duration::from_millis(entry.created_at);
                        let meta = CacheMeta {
                            created_at,
                            ttl,
                            size: entry.data_url.len(),
                        };

                        if self.is_expired(&meta) {
                            let _ = fs::remove_file(file_path);
                            continue;
                        }

                        *total_write += meta.size;
                        items_write.insert(key, meta);
                        loaded += 1;
                    } else {
                        let _ = fs::remove_file(file_path);
                    }
                }
            }
        }
        tracing::info!(
            loaded,
            size_mb = *total_write / 1024 / 1024,
            "图片缓存索引已加载"
        );
    }

    async fn read_data_url_from_file(&self, key: &str) -> Option<String> {
        let path = self.file_path(key);
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(entry) = serde_json::from_str::<FileCacheEntry>(&data) {
                return Some(entry.data_url);
            }
        }
        None
    }

    fn save_to_file(&self, key: &str, image_url: &str, data_url: &str, meta: &CacheMeta) {
        let _ = fs::create_dir_all(&self.cache_dir);
        let ttl_ms = meta.ttl.map(|d| d.as_millis() as i64).unwrap_or(-1);
        let created_at_ms = meta.created_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let entry = FileCacheEntry {
            url: image_url.to_string(),
            data_url: data_url.to_string(),
            created_at: created_at_ms,
            ttl_ms,
        };
        if let Ok(data) = serde_json::to_string(&entry) {
            let _ = fs::write(self.file_path(key), data);
        }
    }

    async fn perform_download(&self, image_url: &str, headers: Option<&HashMap<String, String>>) -> Result<String, String> {
        let mut builder = self.client.get(image_url);
        builder = builder.header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36");
        if let Some(h) = headers {
            for (k, v) in h {
                builder = builder.header(k, v);
            }
        }

        let resp = builder.send().await.map_err(|e| format!("HTTP request error: {}", e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format_upstream_http_error("图片下载", status, &body));
        }

        let content_type = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "image/png".to_string()); // fallback

        let bytes = resp.bytes().await.map_err(|e| format!("Read body error: {}", e))?;
        if bytes.is_empty() {
            return Err("Empty response".to_string());
        }

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(format!("data:{};base64,{}", content_type, b64))
    }

    pub async fn download(&self, image_url: &str, ttl: Option<Duration>, headers: Option<&HashMap<String, String>>) -> String {
        if image_url.is_empty() {
            return String::new();
        }

        let key = self.cache_key(image_url);

        // 1. Check index
        {
            let items_read = self.items.read().await;
            if let Some(meta) = items_read.get(&key) {
                if !self.is_expired(meta) {
                    drop(items_read);
                    if let Some(data_url) = self.read_data_url_from_file(&key).await {
                        return data_url;
                    }
                    // File missing, remove index
                    let mut items_write = self.items.write().await;
                    if let Some(removed) = items_write.remove(&key) {
                        let mut total_write = self.total_size.write().await;
                        *total_write = total_write.saturating_sub(removed.size);
                    }
                }
            }
        }

        // 2. Download remote
        match self.perform_download(image_url, headers).await {
            Ok(data_url) => {
                let size = data_url.len();
                let meta = CacheMeta {
                    created_at: SystemTime::now(),
                    ttl,
                    size,
                };

                let mut items_write = self.items.write().await;
                let mut total_write = self.total_size.write().await;
                *total_write += size;
                items_write.insert(key.clone(), meta.clone());
                drop(items_write);
                drop(total_write);

                self.save_to_file(&key, image_url, &data_url, &meta);
                tracing::debug!(%image_url, size, "图片已缓存");
                data_url
            }
            Err(err) => {
                tracing::warn!(%image_url, error = %err, "图片下载失败");
                String::new()
            }
        }
    }

    pub async fn cleanup(&self) {
        let mut items_write = self.items.write().await;
        let mut total_write = self.total_size.write().await;

        let mut expired_keys = Vec::new();
        for (k, m) in items_write.iter() {
            if self.is_expired(m) {
                expired_keys.push(k.clone());
            }
        }

        let mut expired_count = 0;
        for k in &expired_keys {
            if let Some(removed) = items_write.remove(k) {
                *total_write = total_write.saturating_sub(removed.size);
                let _ = fs::remove_file(self.file_path(k));
                expired_count += 1;
            }
        }

        if *total_write <= self.max_size {
            if expired_count > 0 {
                tracing::info!(
                    expired = expired_count,
                    remaining = items_write.len(),
                    size_mb = *total_write / 1024 / 1024,
                    "图片缓存清理完成"
                );
            }
            return;
        }

        // Evict by LRU (creation time)
        let mut sorted: Vec<(String, CacheMeta)> = items_write.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        sorted.sort_by(|a, b| a.1.created_at.cmp(&b.1.created_at));

        let mut evicted_count = 0;
        for (k, m) in sorted {
            if *total_write <= self.max_size {
                break;
            }
            if items_write.remove(&k).is_some() {
                *total_write = total_write.saturating_sub(m.size);
                let _ = fs::remove_file(self.file_path(&k));
                evicted_count += 1;
            }
        }

        tracing::info!(
            expired = expired_count,
            evicted = evicted_count,
            remaining = items_write.len(),
            size_mb = *total_write / 1024 / 1024,
            "图片缓存清理完成"
        );
    }

    pub fn start_cleanup_ticker(self: Arc<Self>, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.cleanup().await;
            }
        });
    }
}
