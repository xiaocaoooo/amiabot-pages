use std::collections::HashMap;
use std::env;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use url::Url;
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;

pub const GALLERY_TAG_LIST_LIMIT: usize = 500;
pub const GALLERY_IMAGE_LIST_PAGE_SIZE: usize = 100;
pub const GALLERY_TAG_PREVIEW_WORKERS: usize = 8;
pub const GALLERY_LIST_PREVIEW_WIDTH: usize = 320;
pub const GALLERY_LIST_PREVIEW_HEIGHT: usize = 320;
pub const GALLERY_TAG_PREVIEW_WIDTH: usize = 360;
pub const GALLERY_TAG_PREVIEW_HEIGHT: usize = 240;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GalleryTag {
    pub id: i64,
    pub name: String,
    pub created_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GalleryImage {
    pub id: i64,
    pub filename: String,
    pub fid: String,
    pub file_size: i64,
    pub width: i32,
    pub height: i32,
    pub mime_type: String,
    pub phash: i64,
    pub is_animated: bool,
    pub description: String,
    pub created_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GalleryImageWithTags {
    #[serde(flatten)]
    pub image: GalleryImage,
    pub tags: Vec<GalleryTag>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GalleryTagsResponse {
    pub items: Vec<GalleryTag>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GalleryImageListResponse {
    pub items: Vec<GalleryImageWithTags>,
    pub page: i32,
    pub page_size: i32,
    pub total: i64,
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

pub fn build_gallery_render_url(image_id: i64) -> String {
    let base = normalize_http_base(&first_non_empty_env(&["GALLERY_PAGES_RENDER_BASE", "GALLERY_SERVER"]));
    if base.is_empty() {
        return String::new();
    }
    if let Ok(mut parsed) = Url::parse(&base) {
        let path = format!("{}/v1/images/{}/render", parsed.path().trim_end_matches('/'), image_id);
        parsed.set_path(&path);
        parsed.set_query(None);
        parsed.to_string()
    } else {
        String::new()
    }
}

pub fn build_gallery_preview_url(image: &GalleryImageWithTags) -> String {
    build_gallery_render_url(image.image.id)
}

pub fn split_gallery_tags(raw: &str) -> Vec<String> {
    let parts = raw.split(',');
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_lowercase();
        if !seen.contains(&lowered) {
            seen.insert(lowered);
            result.push(trimmed.to_string());
        }
    }
    result
}

pub async fn new_gallery_api_request(path: &str, params: &HashMap<String, String>) -> Result<reqwest::Request, String> {
    let base = normalize_http_base(&env::var("GALLERY_SERVER").unwrap_or_default());
    if base.is_empty() {
        return Err("未配置 GALLERY_SERVER".to_string());
    }
    let mut parsed = Url::parse(&base).map_err(|e| format!("GALLERY_SERVER 无效: {}", e))?;
    let path_str = format!("{}/{}", parsed.path().trim_end_matches('/'), path.trim_start_matches('/'));
    parsed.set_path(&path_str);
    
    let mut query = parsed.query_pairs_mut();
    for (k, v) in params {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            query.append_pair(k, trimmed);
        }
    }
    drop(query);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut builder = client.get(parsed);
    builder = builder.header("Accept", "application/json");

    let token = first_non_empty_env(&["GALLERY_READ_TOKEN", "GALLERY_WRITE_TOKEN"]);
    if !token.is_empty() {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }

    builder.build().map_err(|e| e.to_string())
}

pub async fn call_gallery_json<T: for<'de> Deserialize<'de>>(path: &str, params: &HashMap<String, String>) -> Result<T, String> {
    let req = new_gallery_api_request(path, params).await?;
    let resp = crate::pkg::http_client::execute(req).await.map_err(|e| format!("请求 gallery 服务失败: {}", e))?;
    
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        #[derive(Deserialize)]
        struct ErrResp { error: Option<String> }
        let msg = serde_json::from_str::<ErrResp>(&body)
            .ok()
            .and_then(|r| r.error)
            .unwrap_or(body);
        let msg = if msg.is_empty() {
            format!("gallery 服务返回 HTTP {}", status)
        } else {
            msg
        };
        return Err(msg);
    }

    resp.json::<T>().await.map_err(|e| format!("解析 gallery 响应失败: {}", e))
}

pub async fn fetch_gallery_tags(q: &str, limit: usize) -> Result<Vec<GalleryTag>, String> {
    let mut params = HashMap::new();
    params.insert("q".to_string(), q.trim().to_string());
    params.insert("limit".to_string(), limit.to_string());
    
    let payload = call_gallery_json::<GalleryTagsResponse>("/v1/tags", &params).await?;
    Ok(payload.items)
}

pub async fn fetch_gallery_images_page(tags: &[String], page: i32, page_size: i32) -> Result<GalleryImageListResponse, String> {
    let mut params = HashMap::new();
    params.insert("page".to_string(), page.to_string());
    params.insert("page_size".to_string(), page_size.to_string());
    if !tags.is_empty() {
        params.insert("tags".to_string(), tags.join(","));
    }
    call_gallery_json::<GalleryImageListResponse>("/v1/images", &params).await
}

pub async fn fetch_all_gallery_images(tags: &[String]) -> Result<Vec<GalleryImageWithTags>, String> {
    let mut page = 1;
    let mut items = Vec::new();
    loop {
        let payload = fetch_gallery_images_page(tags, page, GALLERY_IMAGE_LIST_PAGE_SIZE as i32).await?;
        items.extend(payload.items);
        if payload.total == 0 || items.len() as i64 >= payload.total {
            break;
        }
        page += 1;
    }
    Ok(items)
}

pub fn format_gallery_dimensions(image: &GalleryImageWithTags) -> String {
    if image.image.width <= 0 || image.image.height <= 0 {
        "未知尺寸".to_string()
    } else {
        format!("{}×{}", image.image.width, image.image.height)
    }
}

pub fn format_gallery_created_at(image: &GalleryImageWithTags) -> String {
    image.image.created_at.clone().unwrap_or_default()
}
