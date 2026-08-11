use std::collections::{HashMap, HashSet};
use std::env;
use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::pkg::imgcache::DEFAULT_IMG_CACHE;

/// 画廊详情（对齐 gallery-server GalleryDetail）
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GalleryDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 图片详情（对齐 gallery-server ImageDetail）
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ImageDetail {
    pub id: String,
    #[serde(default)]
    pub sha256_hex: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ext: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub size_bytes: i64,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

pub async fn gallery_image_downloader(image_url: &str) -> String {
    DEFAULT_IMG_CACHE.download(image_url, None, None).await
}

pub fn first_non_empty_env(keys: &[&str]) -> String {
    for key in keys {
        if let Ok(val) = env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

pub fn normalize_http_base(host_or_url: &str) -> String {
    let host_or_url = host_or_url.trim();
    if host_or_url.is_empty() {
        return String::new();
    }
    if host_or_url.starts_with("http://") || host_or_url.starts_with("https://") {
        host_or_url.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", host_or_url.trim_end_matches('/'))
    }
}

fn gallery_api_base() -> Result<String, String> {
    let base = normalize_http_base(&env::var("GALLERY_SERVER").unwrap_or_default());
    if base.is_empty() {
        Err("未配置 GALLERY_SERVER".to_string())
    } else {
        Ok(base)
    }
}

fn file_base() -> Result<String, String> {
    let base = normalize_http_base(&first_non_empty_env(&[
        "GALLERY_PAGES_RENDER_BASE",
        "GALLERY_SERVER",
    ]));
    if base.is_empty() {
        Err("未配置 GALLERY_SERVER".to_string())
    } else {
        Ok(base)
    }
}

/// 原图/预览：`GET /files/{image_id}`
pub fn build_gallery_file_url(image_id: &str) -> String {
    let Ok(base) = file_base() else {
        return String::new();
    };
    let id = image_id.trim();
    if id.is_empty() {
        return String::new();
    }
    if let Ok(mut parsed) = Url::parse(&base) {
        let path = format!("{}/files/{}", parsed.path().trim_end_matches('/'), id);
        parsed.set_path(&path);
        parsed.set_query(None);
        parsed.to_string()
    } else {
        String::new()
    }
}

pub fn split_csv_names(raw: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_lowercase();
        if seen.insert(lowered) {
            result.push(trimmed.to_string());
        }
    }
    result
}

pub async fn new_gallery_api_request(
    path: &str,
    params: &HashMap<String, String>,
) -> Result<reqwest::Request, String> {
    let base = gallery_api_base()?;
    let mut parsed = Url::parse(&base).map_err(|e| format!("GALLERY_SERVER 无效: {}", e))?;
    let path_str = format!(
        "{}/{}",
        parsed.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    parsed.set_path(&path_str);

    {
        let mut query = parsed.query_pairs_mut();
        for (k, v) in params {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                query.append_pair(k, trimmed);
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    client
        .get(parsed)
        .header("Accept", "application/json")
        .build()
        .map_err(|e| e.to_string())
}

pub async fn call_gallery_json<T: for<'de> Deserialize<'de>>(
    path: &str,
    params: &HashMap<String, String>,
) -> Result<T, String> {
    let req = new_gallery_api_request(path, params).await?;
    let resp = crate::pkg::http_client::execute(req)
        .await
        .map_err(|e| format!("请求 gallery 服务失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        #[derive(Deserialize)]
        struct ErrResp {
            error: Option<String>,
        }
        let msg = serde_json::from_str::<ErrResp>(&body)
            .ok()
            .and_then(|r| r.error)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                if body.trim().is_empty() {
                    format!("gallery 服务返回 HTTP {}", status)
                } else {
                    body
                }
            });
        return Err(msg);
    }

    resp.json::<T>()
        .await
        .map_err(|e| format!("解析 gallery 响应失败: {}", e))
}

pub async fn fetch_galleries(search: Option<&str>) -> Result<Vec<GalleryDetail>, String> {
    let mut params = HashMap::new();
    if let Some(s) = search {
        let s = s.trim();
        if !s.is_empty() {
            params.insert("search".to_string(), s.to_string());
        }
    }
    call_gallery_json::<Vec<GalleryDetail>>("/galleries", &params).await
}

fn name_or_alias_eq(gallery: &GalleryDetail, key: &str) -> bool {
    let key = key.trim();
    if key.is_empty() {
        return false;
    }
    if gallery.name.eq_ignore_ascii_case(key) {
        return true;
    }
    gallery.aliases.iter().any(|a| a.eq_ignore_ascii_case(key))
}

/// 按画廊名或 alias 精确解析（先 search 再精确过滤）。
pub async fn resolve_gallery(name_or_alias: &str) -> Result<GalleryDetail, String> {
    let key = name_or_alias.trim();
    if key.is_empty() {
        return Err("画廊名为空".to_string());
    }

    let list = fetch_galleries(Some(key)).await?;
    let matches: Vec<GalleryDetail> = list
        .into_iter()
        .filter(|g| name_or_alias_eq(g, key))
        .collect();

    match matches.len() {
        0 => Err(format!("未找到画廊：{}", key)),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(format!(
            "画廊名/alias「{}」匹配到 {} 个结果，请使用更精确的名称",
            key, n
        )),
    }
}

/// 多段 tags：必须全部解析到同一 gallery。
pub async fn resolve_gallery_from_tags(tags: &[String]) -> Result<GalleryDetail, String> {
    if tags.is_empty() {
        return Err("缺少画廊名参数".to_string());
    }
    let first = resolve_gallery(&tags[0]).await?;
    for t in tags.iter().skip(1) {
        let g = resolve_gallery(t).await?;
        if g.id != first.id {
            return Err("仅支持单个画廊（多个名称解析到了不同画廊）".to_string());
        }
    }
    Ok(first)
}

pub async fn fetch_gallery_images(gallery_id: &str) -> Result<Vec<ImageDetail>, String> {
    let id = gallery_id.trim();
    if id.is_empty() {
        return Err("gallery_id 为空".to_string());
    }
    let path = format!("/galleries/{}/images", id);
    call_gallery_json::<Vec<ImageDetail>>(&path, &HashMap::new()).await
}

pub fn format_image_dimensions(image: &ImageDetail) -> String {
    if image.width <= 0 || image.height <= 0 {
        "未知尺寸".to_string()
    } else {
        format!("{}×{}", image.width, image.height)
    }
}

pub fn format_image_created_at(image: &ImageDetail) -> String {
    image.created_at.clone().unwrap_or_default()
}

pub fn format_aliases(aliases: &[String]) -> String {
    aliases
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}
