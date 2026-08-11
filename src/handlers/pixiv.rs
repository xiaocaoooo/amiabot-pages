use axum::{
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use gif::{Encoder, Frame, Repeat};
use image::{GenericImageView, ImageFormat};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use url::Url;
use zip::ZipArchive;

use crate::handlers::{format_upstream_http_error, render_html};
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;

const PIXIV_BINARY_CACHE_CONTROL: &str = "public, max-age=86400";
const PIXIV_UGOIRA_ZIP_MAX_BYTES: u64 = 64 << 20;
const PIXIV_PREVIEW_LIMIT: usize = 6;

const PIXIV_HASH_SALT: &str = "28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c";
const PIXIV_USER_AGENT: &str = "PixivAndroidApp/5.0.166 (Android 10.0; Pixel C)";
const PIXIV_IMAGE_AGENT: &str = "PixivIOSApp/5.8.0";
const PIXIV_ACCEPT_LANG: &str = "zh-CN";
const PIXIV_APP_OS: &str = "Android";
const PIXIV_APP_OS_VERSION: &str = "Android 10.0";
const PIXIV_APP_VERSION: &str = "5.0.166";
const PIXIV_CLIENT_ID: &str = "MOBrBDS8blbauoSck0ZfDbtuzpyT";
const PIXIV_CLIENT_SECRET: &str = "lsACyCD94FhDUtGTXi3QzcFE2uU1hqtDaKeqrdwj";
const PIXIV_TOKEN_ERROR_MSG: &str = "缺少 Pixiv token，请在请求头传 Authorization: Bearer <access_token>，或配置 PIXIV_ACCESS_TOKEN / PIXIV_REFRESH_TOKEN";

static PIXIV_BREAK_TAG_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static PIXIV_PARAGRAPH_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</?p[^>]*>").unwrap());
static PIXIV_LIST_ITEM_START: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<li[^>]*>").unwrap());
static PIXIV_LIST_ITEM_END: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</li>").unwrap());
static PIXIV_ANY_TAG_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").unwrap());

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum PixivTokenSource {
    None,
    Header,
    EnvAccess,
    RefreshCache,
    Refresh,
}

struct PixivTokenCache {
    access_token: String,
    expires_at: Option<SystemTime>,
}

static CACHED_PIXIV_TOKEN: Lazy<RwLock<PixivTokenCache>> = Lazy::new(|| {
    RwLock::new(PixivTokenCache {
        access_token: String::new(),
        expires_at: None,
    })
});

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub struct PixivQuery {
    pub id: Option<String>,
    pub pid: Option<String>,
    pub url: Option<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PixivPageResponse {
    pub Error: Option<String>,
    pub Illust: Option<PixivIllustView>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PixivIllustView {
    pub ID: i32,
    pub Title: String,
    pub Type: String,
    pub TypeRaw: String,
    pub AuthorName: String,
    pub AuthorAccount: String,
    pub AuthorAvatar: String,
    pub MainImage: String,
    pub Previews: Vec<PixivPreviewView>,
    pub PageCount: i32,
    pub DisplayedPreviews: usize,
    pub HasMorePreviews: bool,
    pub RemainingPreviews: i32,
    pub Dimensions: String,
    pub Width: i32,
    pub Height: i32,
    pub TotalView: String,
    pub TotalBookmarks: String,
    pub TotalComments: String,
    pub CreateAt: String,
    pub Caption: String,
    pub Tags: Vec<PixivTagView>,
    pub SeriesID: i32,
    pub SeriesTitle: String,
    pub HasSeries: bool,
    pub IsAIGenerated: bool,
    pub AIGeneratedLabel: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PixivPreviewView {
    pub Index: usize,
    pub Image: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PixivTagView {
    pub Name: String,
    pub TranslatedName: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PixivIllust {
    pub id: i32,
    pub title: String,
    #[serde(rename = "type")]
    pub illust_type: String,
    pub caption: String,
    pub create_date: String,
    pub page_count: i32,
    pub width: i32,
    pub height: i32,
    pub user: PixivUser,
    pub tags: Vec<PixivTag>,
    #[serde(default)]
    pub meta_single_page: MetaSinglePage,
    #[serde(default)]
    pub meta_pages: Vec<MetaPage>,
    pub total_view: i32,
    pub total_bookmarks: i32,
    pub total_comments: i32,
    pub illust_ai_type: i32,
    pub series: Option<PixivIllustSeriesRef>,
    #[serde(default)]
    pub image_urls: PixivImageURLs,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct PixivImageURLs {
    pub square_medium: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct PixivIllustSeriesRef {
    pub id: i32,
    pub title: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct MetaSinglePage {
    pub original_image_url: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct MetaPage {
    pub image_urls: ImageUrls,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct ImageUrls {
    pub square_medium: Option<String>,
    pub medium: Option<String>,
    pub large: Option<String>,
    pub original: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct PixivUser {
    pub id: i32,
    pub name: String,
    pub account: String,
    pub profile_image_urls: PixivProfileImageURLs,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct PixivProfileImageURLs {
    pub medium: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct PixivTag {
    pub name: String,
    pub translated_name: Option<String>,
}

#[derive(Deserialize, Debug)]
struct PixivUgoiraMetadata {
    ugoira_metadata: UgoiraMetadata,
}

#[derive(Deserialize, Debug)]
struct UgoiraMetadata {
    zip_urls: ZipUrls,
    frames: Vec<UgoiraFrame>,
}

#[derive(Deserialize, Debug)]
struct ZipUrls {
    medium: String,
}

#[derive(Deserialize, Debug)]
struct UgoiraFrame {
    file: String,
    delay: i32,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct PixivMediaManifest {
    pub pid: i32,
    pub title: String,
    pub r#type: String,
    pub page_count: i32,
    pub items: Vec<PixivMediaItem>,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct PixivMediaItem {
    pub index: usize,
    pub kind: String,
    pub path: String,
}

#[derive(Deserialize, Debug)]
struct PixivOAuthResponse {
    response: PixivOAuthToken,
}

#[derive(Deserialize, Debug)]
struct PixivOAuthToken {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize, Debug)]
struct PixivIllustDetailResponse {
    illust: PixivIllust,
}

#[derive(Deserialize, Debug)]
struct PixivErrorResponse {
    error: PixivErrorDetail,
}

#[derive(Deserialize, Debug)]
struct PixivErrorDetail {
    user_message: Option<String>,
    message: Option<String>,
    reason: Option<String>,
}

pub fn md5_hex(s: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

fn pixiv_client_time() -> String {
    let now = chrono::Utc::now();
    now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

fn apply_pixiv_common_headers(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let client_time = pixiv_client_time();
    let client_hash = md5_hex(&format!("{}{}", client_time, PIXIV_HASH_SALT));

    builder
        .header("X-Client-Time", client_time)
        .header("X-Client-Hash", client_hash)
        .header("User-Agent", PIXIV_USER_AGENT)
        .header("Accept-Language", PIXIV_ACCEPT_LANG)
        .header("App-OS", PIXIV_APP_OS)
        .header("App-OS-Version", PIXIV_APP_OS_VERSION)
        .header("App-Version", PIXIV_APP_VERSION)
}

pub fn pixiv_image_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert(
        "Referer".to_string(),
        "https://app-api.pixiv.net/".to_string(),
    );
    h.insert("User-Agent".to_string(), PIXIV_IMAGE_AGENT.to_string());
    h
}

async fn resolve_pixiv_access_token(
    headers_opt: Option<&HeaderMap>,
) -> Result<(String, PixivTokenSource), String> {
    // 1. Try from request Authorization header
    if let Some(headers) = headers_opt {
        if let Some(auth_val) = headers.get(axum::http::header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_val.to_str() {
                if auth_str.to_lowercase().starts_with("bearer ") {
                    let token = auth_str[7..].trim().to_string();
                    if !token.is_empty() {
                        return Ok((token, PixivTokenSource::Header));
                    }
                }
            }
        }
    }

    // 2. Try from global mem cache
    {
        let cache = CACHED_PIXIV_TOKEN.read().await;
        if !cache.access_token.is_empty() {
            if let Some(exp) = cache.expires_at {
                if exp > SystemTime::now() + Duration::from_secs(60) {
                    return Ok((cache.access_token.clone(), PixivTokenSource::RefreshCache));
                }
            }
        }
    }

    // 3. Try from env access token
    if let Ok(env_token) = env::var("PIXIV_ACCESS_TOKEN") {
        let trimmed = env_token.trim().to_string();
        if !trimmed.is_empty() {
            return Ok((trimmed, PixivTokenSource::EnvAccess));
        }
    }

    // 4. Try refresh token from env
    if let Ok(refresh_token) = env::var("PIXIV_REFRESH_TOKEN") {
        let trimmed = refresh_token.trim().to_string();
        if !trimmed.is_empty() {
            let refreshed = refresh_pixiv_access_token(&trimmed).await?;
            return Ok((refreshed, PixivTokenSource::Refresh));
        }
    }

    Err(PIXIV_TOKEN_ERROR_MSG.to_string())
}

async fn refresh_pixiv_access_token(refresh_token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = "https://oauth.secure.pixiv.net/v1/auth/token";

    let mut params = HashMap::new();
    params.insert("client_id", PIXIV_CLIENT_ID);
    params.insert("client_secret", PIXIV_CLIENT_SECRET);
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_token);

    let req_builder = client.post(url).form(&params);
    let req_builder = apply_pixiv_common_headers(req_builder);

    let resp = crate::pkg::http_client::send(req_builder)
        .await
        .map_err(|e| format!("刷新 Pixiv token 失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.bytes().await.map_err(|e| e.to_string())?;
        return Err(parse_pixiv_api_error(status.as_u16() as i32, &body));
    }

    let payload = resp
        .json::<PixivOAuthResponse>()
        .await
        .map_err(|e| format!("解析 Pixiv token 响应失败: {}", e))?;

    let access_token = payload.response.access_token.clone();
    let expires_in = payload.response.expires_in;

    let mut cache = CACHED_PIXIV_TOKEN.write().await;
    cache.access_token = access_token.clone();
    cache.expires_at = Some(SystemTime::now() + Duration::from_secs(expires_in));

    Ok(access_token)
}

fn parse_pixiv_api_error(status_code: i32, body: &[u8]) -> String {
    if let Ok(payload) = serde_json::from_slice::<PixivErrorResponse>(body) {
        let msg = payload
            .error
            .user_message
            .or(payload.error.message)
            .or(payload.error.reason);
        if let Some(m) = msg {
            return format!("Pixiv API 错误: {} (code={})", m, status_code);
        }
    }

    let trimmed = String::from_utf8_lossy(body);
    let trimmed = trimmed.trim();
    let limit = if trimmed.len() > 200 {
        &trimmed[..200]
    } else {
        trimmed
    };
    format!("Pixiv API 返回异常状态码: {}, Body: {}", status_code, limit)
}

async fn fetch_pixiv_illust_detail(access_token: &str, pid: i32) -> Result<PixivIllust, String> {
    let client = reqwest::Client::new();
    let detail_url = format!(
        "https://app-api.pixiv.net/v1/illust/detail?filter=for_android&illust_id={}",
        pid
    );

    let req_builder = client
        .get(&detail_url)
        .header("Authorization", format!("Bearer {}", access_token));
    let req_builder = apply_pixiv_common_headers(req_builder);

    let resp = crate::pkg::http_client::send(req_builder)
        .await
        .map_err(|e| format!("请求 Pixiv 插画详情失败: {}", e))?;

    let status = resp.status();
    let body = resp.bytes().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(parse_pixiv_api_error(status.as_u16() as i32, &body));
    }

    let payload: PixivIllustDetailResponse =
        serde_json::from_slice(&body).map_err(|e| format!("解析 Pixiv 插画详情失败: {}", e))?;

    if payload.illust.id <= 0 {
        return Err(format!("未找到插画 PID: {}", pid));
    }

    Ok(payload.illust)
}

async fn get_pixiv_illust_detail(
    pid: i32,
    headers_opt: Option<&HeaderMap>,
) -> Result<PixivIllust, String> {
    let (access_token, source) = match resolve_pixiv_access_token(headers_opt).await {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    match fetch_pixiv_illust_detail(&access_token, pid).await {
        Ok(ill) => Ok(ill),
        Err(err) => {
            // If OAuth related error, and we did not use custom Header, try to refresh
            let is_oauth_err =
                err.to_lowercase().contains("oauth") || err.to_lowercase().contains("token");
            if is_oauth_err && source != PixivTokenSource::Header {
                if let Ok(refresh_token) = env::var("PIXIV_REFRESH_TOKEN") {
                    let trimmed = refresh_token.trim().to_string();
                    if !trimmed.is_empty() {
                        {
                            let mut cache = CACHED_PIXIV_TOKEN.write().await;
                            cache.access_token.clear();
                            cache.expires_at = None;
                        }
                        if let Ok(refreshed_token) = refresh_pixiv_access_token(&trimmed).await {
                            if let Ok(retry_ill) =
                                fetch_pixiv_illust_detail(&refreshed_token, pid).await
                            {
                                return Ok(retry_ill);
                            }
                        }
                    }
                }
            }
            Err(err)
        }
    }
}

async fn fetch_pixiv_ugoira_metadata(
    access_token: &str,
    pid: i32,
) -> Result<PixivUgoiraMetadata, String> {
    let client = reqwest::Client::new();
    let metadata_url = format!(
        "https://app-api.pixiv.net/v1/ugoira/metadata?illust_id={}",
        pid
    );

    let req_builder = client
        .get(&metadata_url)
        .header("Authorization", format!("Bearer {}", access_token));
    let req_builder = apply_pixiv_common_headers(req_builder);

    let resp = crate::pkg::http_client::send(req_builder)
        .await
        .map_err(|e| format!("请求 Pixiv 动图元数据失败: {}", e))?;

    let status = resp.status();
    let body = resp.bytes().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(parse_pixiv_api_error(status.as_u16() as i32, &body));
    }

    serde_json::from_slice(&body).map_err(|e| format!("解析 Pixiv 动图元数据失败: {}", e))
}

async fn get_pixiv_ugoira_metadata_with_refresh(
    pid: i32,
    headers_opt: Option<&HeaderMap>,
) -> Result<PixivUgoiraMetadata, String> {
    let (access_token, source) = match resolve_pixiv_access_token(headers_opt).await {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    match fetch_pixiv_ugoira_metadata(&access_token, pid).await {
        Ok(meta) => Ok(meta),
        Err(err) => {
            let is_oauth_err =
                err.to_lowercase().contains("oauth") || err.to_lowercase().contains("token");
            if is_oauth_err && source != PixivTokenSource::Header {
                if let Ok(refresh_token) = env::var("PIXIV_REFRESH_TOKEN") {
                    let trimmed = refresh_token.trim().to_string();
                    if !trimmed.is_empty() {
                        {
                            let mut cache = CACHED_PIXIV_TOKEN.write().await;
                            cache.access_token.clear();
                            cache.expires_at = None;
                        }
                        if let Ok(refreshed_token) = refresh_pixiv_access_token(&trimmed).await {
                            if let Ok(retry_meta) =
                                fetch_pixiv_ugoira_metadata(&refreshed_token, pid).await
                            {
                                return Ok(retry_meta);
                            }
                        }
                    }
                }
            }
            Err(err)
        }
    }
}

pub fn parse_pid(q: &PixivQuery) -> Result<i32, String> {
    let pid_str = q
        .pid
        .as_ref()
        .or(q.id.as_ref())
        .map(|s| s.trim())
        .unwrap_or("");
    if pid_str.is_empty() {
        return Err("缺少插画 PID 参数".to_string());
    }
    pid_str
        .parse::<i32>()
        .map_err(|_| format!("无效的插画 PID: {}", pid_str))
}

pub async fn illust_info_handler(
    Query(q): Query<PixivQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => {
            return render_html(
                "pixiv/illust.html",
                PixivPageResponse {
                    Error: Some(err),
                    Illust: None,
                },
            )
            .into_response()
        }
    };

    let detail = match get_pixiv_illust_detail(pid, Some(&headers)).await {
        Ok(ill) => ill,
        Err(err) => {
            return render_html(
                "pixiv/illust.html",
                PixivPageResponse {
                    Error: Some(err),
                    Illust: None,
                },
            )
            .into_response()
        }
    };

    let page_data = build_pixiv_illust_page_data(&detail).await;

    render_html(
        "pixiv/illust.html",
        PixivPageResponse {
            Error: None,
            Illust: Some(page_data),
        },
    )
    .into_response()
}

pub async fn illust_media_handler(
    Query(q): Query<PixivQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    let illust = match get_pixiv_illust_detail(pid, Some(&headers)).await {
        Ok(ill) => ill,
        Err(err) => {
            tracing::warn!(error = %err, "pixiv 上游失败");
            return (StatusCode::BAD_GATEWAY, err).into_response();
        }
    };

    match build_media_manifest(&illust) {
        Ok(manifest) => (StatusCode::OK, axum::Json(manifest)).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

pub async fn pixiv_image_proxy_handler(Query(q): Query<PixivQuery>) -> impl IntoResponse {
    let raw_url = match &q.url {
        Some(u) => u.trim(),
        None => return (StatusCode::BAD_REQUEST, "缺少图片 URL 参数").into_response(),
    };

    let parsed_url = match Url::parse(raw_url) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "图片 URL 无效").into_response(),
    };

    let host = parsed_url.host_str().unwrap_or_default().to_lowercase();
    if host != "pximg.net" && !host.ends_with(".pximg.net") {
        return (StatusCode::BAD_REQUEST, "仅支持代理 pximg.net 图片").into_response();
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    let req_builder = client
        .get(parsed_url.clone())
        .header("Referer", "https://www.pixiv.net/");

    let resp = match crate::pkg::http_client::send(req_builder).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("请求 Pixiv 图片失败: {}", e),
            )
                .into_response()
        }
    };

    if !resp.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            format!("Pixiv 图片下载失败: HTTP {}", resp.status()),
        )
            .into_response();
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("读取 Pixiv 图像流失败: {}", e),
            )
                .into_response()
        }
    };

    let filename = parsed_url
        .path_segments()
        .and_then(|s| s.last())
        .unwrap_or("image.png");

    let mut headers = HeaderMap::new();
    if let Ok(v) = content_type.parse() {
        headers.insert(axum::http::header::CONTENT_TYPE, v);
    }
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        PIXIV_BINARY_CACHE_CONTROL.parse().unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("inline; filename=\"{}\"", filename)
            .parse()
            .unwrap(),
    );

    (StatusCode::OK, headers, bytes).into_response()
}

pub async fn pixiv_ugoira_gif_handler(
    Query(q): Query<PixivQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    let metadata = match get_pixiv_ugoira_metadata_with_refresh(pid, Some(&headers)).await {
        Ok(m) => m,
        Err(err) => return (StatusCode::BAD_GATEWAY, err).into_response(),
    };

    let zip_url = metadata.ugoira_metadata.zip_urls.medium.trim();
    if zip_url.is_empty() {
        return (StatusCode::BAD_GATEWAY, "Pixiv 动图未返回可用 zip 资源").into_response();
    }

    let zip_data = match download_pixiv_binary(zip_url, PIXIV_UGOIRA_ZIP_MAX_BYTES).await {
        Ok(d) => d,
        Err(err) => return (StatusCode::BAD_GATEWAY, err).into_response(),
    };

    let gif_data = match convert_ugoira_to_gif(&zip_data, &metadata.ugoira_metadata.frames) {
        Ok(g) => g,
        Err(err) => return (StatusCode::BAD_GATEWAY, err).into_response(),
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        PIXIV_BINARY_CACHE_CONTROL.parse().unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("inline; filename=\"pixiv-ugoira-{}.gif\"", pid)
            .parse()
            .unwrap(),
    );

    (StatusCode::OK, headers, gif_data).into_response()
}

async fn download_pixiv_binary(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::new();
    let resp =
        crate::pkg::http_client::send(client.get(url).header("Referer", "https://www.pixiv.net/"))
            .await
            .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error(
            "Pixiv resource download",
            status,
            &body,
        ));
    }

    let body = resp.bytes().await.map_err(|e| e.to_string())?;
    if body.len() as u64 > max_bytes {
        return Err("Pixiv resource exceeds limit".to_string());
    }

    Ok(body.to_vec())
}

fn build_media_manifest(illust: &PixivIllust) -> Result<PixivMediaManifest, String> {
    let r_type = illust.illust_type.clone();
    let mut manifest = PixivMediaManifest {
        pid: illust.id,
        title: illust.title.clone(),
        r#type: r_type.clone(),
        page_count: illust.page_count,
        items: Vec::new(),
    };

    if r_type.to_lowercase() == "ugoira" {
        manifest.items.push(PixivMediaItem {
            index: 0,
            kind: "gif".to_string(),
            path: format!("/pixiv/ugoira/gif?pid={}", illust.id),
        });
        if manifest.page_count <= 0 {
            manifest.page_count = 1;
        }
        return Ok(manifest);
    }

    let urls = extract_original_urls(illust);
    for (i, url) in urls.into_iter().enumerate() {
        let escaped_url = urlencoding::encode(&url);
        manifest.items.push(PixivMediaItem {
            index: i,
            kind: "image".to_string(),
            path: format!("/pixiv/image?url={}", escaped_url),
        });
    }

    if manifest.page_count <= 0 {
        manifest.page_count = manifest.items.len() as i32;
    }

    Ok(manifest)
}

fn extract_original_urls(illust: &PixivIllust) -> Vec<String> {
    if illust.page_count > 1 {
        let mut list = Vec::new();
        for page in &illust.meta_pages {
            if let Some(ref o) = page.image_urls.original {
                list.push(o.clone());
            }
        }
        if !list.is_empty() {
            return list;
        }
    }

    if let Some(ref o) = illust.meta_single_page.original_image_url {
        return vec![o.clone()];
    }

    let mut list = Vec::new();
    for page in &illust.meta_pages {
        if let Some(ref o) = page.image_urls.original {
            list.push(o.clone());
        }
    }
    list
}

fn convert_ugoira_to_gif(zip_data: &[u8], frames: &[UgoiraFrame]) -> Result<Vec<u8>, String> {
    if zip_data.is_empty() {
        return Err("Ugoira ZIP is empty".to_string());
    }
    if frames.is_empty() {
        return Err("Ugoira frames empty".to_string());
    }

    let cursor = Cursor::new(zip_data);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid ZIP: {}", e))?;

    let mut out_buffer = Vec::new();

    // First frame to define size
    let first_frame_name = frames[0].file.trim();
    let mut zip_file = archive.by_name(first_frame_name).map_err(|_| {
        let alt = Path::new(first_frame_name)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        format!("Frame {} missing", alt)
    })?;

    let mut img_bytes = Vec::new();
    std::io::copy(&mut zip_file, &mut img_bytes).map_err(|e| e.to_string())?;
    drop(zip_file); // release archive borrow

    let img = image::load_from_memory(&img_bytes)
        .map_err(|e| format!("Decode first frame failed: {}", e))?;
    let (width, height) = img.dimensions();

    let mut encoder = Encoder::new(&mut out_buffer, width as u16, height as u16, &[])
        .map_err(|e| e.to_string())?;
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| e.to_string())?;

    for frame in frames {
        let name = frame.file.trim();
        let mut file = archive.by_name(name).map_err(|_| {
            let alt = Path::new(name).file_name().unwrap().to_str().unwrap();
            format!("Frame {} missing", alt)
        })?;

        let mut b = Vec::new();
        std::io::copy(&mut file, &mut b).map_err(|e| e.to_string())?;
        drop(file); // release archive borrow

        let frame_img = image::load_from_memory_with_format(&b, ImageFormat::Png)
            .or_else(|_| image::load_from_memory_with_format(&b, ImageFormat::Jpeg))
            .map_err(|e| format!("Frame decode failed: {}", e))?;

        let rgba = frame_img.to_rgba8();
        let mut pixels = rgba.clone().into_raw();

        let delay_hundredths = (frame.delay / 10).max(1) as u16;

        let mut gif_frame = Frame::from_rgba_speed(width as u16, height as u16, &mut pixels, 10);
        gif_frame.delay = delay_hundredths;

        encoder.write_frame(&gif_frame).map_err(|e| e.to_string())?;
    }

    drop(encoder);
    Ok(out_buffer)
}

fn load_pixiv_tag_blacklist() -> Option<HashSet<String>> {
    let raw = env::var("PIXIV_TAG_BLACKLIST").unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let mut blacklist = HashSet::new();
    // split by comma, semicolon, newline, etc.
    let tokens = raw.split(|c| {
        c == ',' || c == '，' || c == ';' || c == '；' || c == '\n' || c == '\r' || c == '\t'
    });
    for token in tokens {
        let normalized = normalize_pixiv_tag_value(token);
        if !normalized.is_empty() {
            blacklist.insert(normalized);
        }
    }

    if blacklist.is_empty() {
        None
    } else {
        Some(blacklist)
    }
}

fn normalize_pixiv_tag_value(s: &str) -> String {
    s.trim().to_lowercase()
}

fn is_pixiv_tag_blacklisted(tag: &PixivTag, blacklist: &Option<HashSet<String>>) -> bool {
    let blacklist = match blacklist {
        Some(b) => b,
        None => return false,
    };

    if blacklist.contains(&normalize_pixiv_tag_value(&tag.name)) {
        return true;
    }
    if let Some(ref trans) = tag.translated_name {
        if blacklist.contains(&normalize_pixiv_tag_value(trans)) {
            return true;
        }
    }
    false
}

async fn download_pixiv_image(url: &str) -> String {
    let headers = pixiv_image_headers();
    DEFAULT_IMG_CACHE.download(url, None, Some(&headers)).await
}

fn pick_pixiv_main_image_url(illust: &PixivIllust) -> String {
    if !illust.meta_pages.is_empty() {
        let page = &illust.meta_pages[0];
        return first_non_empty(&[
            page.image_urls.large.as_deref(),
            page.image_urls.original.as_deref(),
            page.image_urls.medium.as_deref(),
        ]);
    }

    first_non_empty(&[
        illust.meta_single_page.original_image_url.as_deref(),
        illust.image_urls.large.as_deref(),
        illust.image_urls.medium.as_deref(),
    ])
}

fn extract_pixiv_preview_urls(illust: &PixivIllust) -> Vec<String> {
    let mut urls = Vec::new();
    for page in &illust.meta_pages {
        let image_url = first_non_empty(&[
            page.image_urls.large.as_deref(),
            page.image_urls.original.as_deref(),
            page.image_urls.medium.as_deref(),
        ]);
        if !image_url.is_empty() {
            urls.push(image_url);
        }
    }
    if !urls.is_empty() {
        return urls;
    }

    let main = pick_pixiv_main_image_url(illust);
    if !main.is_empty() {
        vec![main]
    } else {
        vec![]
    }
}

fn normalize_pixiv_caption(raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return String::new();
    }

    let mut text = PIXIV_BREAK_TAG_PATTERN.replace_all(text, "\n").into_owned();
    text = PIXIV_PARAGRAPH_PATTERN
        .replace_all(&text, "\n")
        .into_owned();
    text = PIXIV_LIST_ITEM_START.replace_all(&text, "• ").into_owned();
    text = PIXIV_LIST_ITEM_END.replace_all(&text, "\n").into_owned();
    text = PIXIV_ANY_TAG_PATTERN.replace_all(&text, "").into_owned();

    // Html entity unescape helper
    let text = html_escape::decode_html_entities(&text).into_owned();
    let text = text.replace("\r\n", "\n").replace('\r', "\n");

    let lines = text.split('\n');
    let mut normalized = Vec::new();
    let mut last_blank = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if last_blank {
                continue;
            }
            last_blank = true;
            normalized.push("");
            continue;
        }
        last_blank = false;
        normalized.push(trimmed);
    }

    normalized.join("\n").trim().to_string()
}

fn format_pixiv_count(v: i32) -> String {
    let value = v as i64;
    if value >= 100_000_000 {
        format!("{:.1}亿", value as f64 / 100_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1}万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

fn format_pixiv_dimensions(width: i32, height: i32) -> String {
    if width <= 0 || height <= 0 {
        "-".to_string()
    } else {
        format!("{}x{}", width, height)
    }
}

fn format_pixiv_create_date(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    } else {
        raw.to_string()
    }
}

fn pixiv_work_type_label(kind: &str) -> String {
    match kind.trim().to_lowercase().as_str() {
        "illust" => "插画".to_string(),
        "manga" => "漫画".to_string(),
        "ugoira" => "动图".to_string(),
        _ => {
            if kind.trim().is_empty() {
                "未知".to_string()
            } else {
                kind.to_string()
            }
        }
    }
}

fn first_non_empty(values: &[Option<&str>]) -> String {
    for val in values {
        if let Some(v) = val {
            if !v.trim().is_empty() {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

async fn build_pixiv_illust_page_data(illust: &PixivIllust) -> PixivIllustView {
    let preview_urls = extract_pixiv_preview_urls(illust);
    let mut page_count = illust.page_count;
    if page_count <= 0 {
        if !preview_urls.is_empty() {
            page_count = preview_urls.len() as i32;
        } else {
            page_count = 1;
        }
    }

    let mut main_image_url = pick_pixiv_main_image_url(illust);
    if main_image_url.is_empty() && !preview_urls.is_empty() {
        main_image_url = preview_urls[0].clone();
    }

    let mut displayed_previews = preview_urls.len();
    if displayed_previews > PIXIV_PREVIEW_LIMIT {
        displayed_previews = PIXIV_PREVIEW_LIMIT;
    }

    let mut previews = Vec::with_capacity(displayed_previews);
    for i in 0..displayed_previews {
        let img_data_url = download_pixiv_image(&preview_urls[i]).await;
        previews.push(PixivPreviewView {
            Index: i + 1,
            Image: img_data_url,
        });
    }

    let blacklist = load_pixiv_tag_blacklist();
    let mut tags = Vec::new();
    for tag in &illust.tags {
        if is_pixiv_tag_blacklisted(tag, &blacklist) {
            continue;
        }
        tags.push(PixivTagView {
            Name: tag.name.clone(),
            TranslatedName: tag.translated_name.clone(),
        });
    }

    let mut remaining_previews = page_count - displayed_previews as i32;
    if remaining_previews < 0 {
        remaining_previews = 0;
    }

    let author_avatar = download_pixiv_image(&illust.user.profile_image_urls.medium).await;
    let main_image = download_pixiv_image(&main_image_url).await;

    let mut view = PixivIllustView {
        ID: illust.id,
        Title: illust.title.clone(),
        Type: pixiv_work_type_label(&illust.illust_type),
        TypeRaw: illust.illust_type.clone(),
        AuthorName: illust.user.name.clone(),
        AuthorAccount: illust.user.account.clone(),
        AuthorAvatar: author_avatar,
        MainImage: main_image,
        Previews: previews,
        PageCount: page_count,
        DisplayedPreviews: displayed_previews,
        HasMorePreviews: remaining_previews > 0,
        RemainingPreviews: remaining_previews,
        Dimensions: format_pixiv_dimensions(illust.width, illust.height),
        Width: illust.width,
        Height: illust.height,
        TotalView: format_pixiv_count(illust.total_view),
        TotalBookmarks: format_pixiv_count(illust.total_bookmarks),
        TotalComments: format_pixiv_count(illust.total_comments),
        CreateAt: format_pixiv_create_date(&illust.create_date),
        Caption: normalize_pixiv_caption(&illust.caption),
        Tags: tags,
        SeriesID: 0,
        SeriesTitle: String::new(),
        HasSeries: false,
        IsAIGenerated: illust.illust_ai_type == 2,
        AIGeneratedLabel: "AI生成作品".to_string(),
    };

    if let Some(ref ser) = illust.series {
        if ser.id > 0 {
            view.HasSeries = true;
            view.SeriesID = ser.id;
            view.SeriesTitle = ser.title.clone();
        }
    }

    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pixiv_caption() {
        let raw = "<p>Hello World!</p><br/><li>Item 1</li><li>Item 2</li><a href=\"#\">Link</a>";
        let cleaned = normalize_pixiv_caption(raw);
        assert!(cleaned.contains("Hello World!"));
        assert!(cleaned.contains("• Item 1"));
        assert!(cleaned.contains("• Item 2"));
        assert!(!cleaned.contains("<p>"));
    }

    #[test]
    fn test_format_pixiv_count() {
        assert_eq!(format_pixiv_count(150), "150");
        assert_eq!(format_pixiv_count(12345), "1.2万");
        assert_eq!(format_pixiv_count(234567890), "2.3亿");
    }

    #[test]
    fn test_md5_hex() {
        let hash = md5_hex("hello");
        assert_eq!(hash, "5d41402abc4b2a76b9719d911017c592");
    }
}
