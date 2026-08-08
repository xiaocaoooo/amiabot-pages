use axum::{
    extract::Query,
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;
use zip::ZipArchive;
use image::{GenericImageView, ImageFormat};
use gif::{Frame, Encoder, Repeat};
use url::Url;

use crate::handlers::render_html;

const PIXIV_BINARY_CACHE_CONTROL: &str = "public, max-age=86400";
const PIXIV_UGOIRA_ZIP_MAX_BYTES: u64 = 64 << 20;

#[derive(Deserialize, Debug)]
pub struct PixivQuery {
    pub id: Option<String>,
    pub pid: Option<String>,
    pub url: Option<String>,
}

#[derive(Serialize)]
pub struct PixivPageResponse {
    pub Error: Option<String>,
    pub Illust: Option<PixivIllustView>,
}

#[derive(Serialize)]
pub struct PixivIllustView {
    pub ID: i32,
    pub Title: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PixivIllust {
    pub id: i32,
    pub title: String,
    #[serde(rename = "type")]
    pub illust_type: String,
    pub page_count: i32,
    #[serde(default)]
    pub meta_single_page: MetaSinglePage,
    #[serde(default)]
    pub meta_pages: Vec<MetaPage>,
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
    pub original: Option<String>,
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
pub struct PixivMediaManifest {
    pub pid: i32,
    pub title: String,
    pub r#type: String,
    pub page_count: i32,
    pub items: Vec<PixivMediaItem>,
}

#[derive(Serialize, Debug)]
pub struct PixivMediaItem {
    pub index: usize,
    pub kind: String,
    pub path: String,
}

pub fn pixiv_image_headers() -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Referer".to_string(), "https://www.pixiv.net/".to_string());
    h
}

fn parse_pid(q: &PixivQuery) -> Result<i32, String> {
    let pid_str = q.pid.as_ref().or(q.id.as_ref()).map(|s| s.trim()).unwrap_or("");
    if pid_str.is_empty() {
        return Err("缺少插画 PID 参数".to_string());
    }
    pid_str.parse::<i32>().map_err(|_| format!("无效的插画 PID: {}", pid_str))
}

pub async fn illust_info_handler(Query(q): Query<PixivQuery>) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => return render_html("pixiv/illust.html", PixivPageResponse { Error: Some(err), Illust: None }).into_response(),
    };

    render_html("pixiv/illust.html", PixivPageResponse {
        Error: None,
        Illust: Some(PixivIllustView {
            ID: pid,
            Title: format!("Pixiv Illust #{}", pid),
        }),
    }).into_response()
}

pub async fn illust_media_handler(Query(q): Query<PixivQuery>) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    let illust = match get_pixiv_illust_detail(pid).await {
        Ok(ill) => ill,
        Err(err) => return (StatusCode::BAD_GATEWAY, err).into_response(),
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

    let req_builder = client.get(parsed_url.clone())
        .header("Referer", "https://www.pixiv.net/");

    let resp = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("请求 Pixiv 图片失败: {}", e)).into_response(),
    };

    if !resp.status().is_success() {
        return (StatusCode::BAD_GATEWAY, format!("Pixiv 图片下载失败: HTTP {}", resp.status())).into_response();
    }

    let content_type = resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("读取 Pixiv 图像流失败: {}", e)).into_response(),
    };

    let filename = parsed_url.path_segments()
        .and_then(|s| s.last())
        .unwrap_or("image.png");

    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::CACHE_CONTROL, PIXIV_BINARY_CACHE_CONTROL.parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("inline; filename=\"{}\"", filename).parse().unwrap(),
    );

    (StatusCode::OK, headers, bytes).into_response()
}

pub async fn pixiv_ugoira_gif_handler(Query(q): Query<PixivQuery>) -> impl IntoResponse {
    let pid = match parse_pid(&q) {
        Ok(id) => id,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    let metadata = match get_pixiv_ugoira_metadata(pid).await {
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
    headers.insert(axum::http::header::CACHE_CONTROL, PIXIV_BINARY_CACHE_CONTROL.parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("inline; filename=\"pixiv-ugoira-{}.gif\"", pid).parse().unwrap(),
    );

    (StatusCode::OK, headers, gif_data).into_response()
}

async fn get_pixiv_illust_detail(pid: i32) -> Result<PixivIllust, String> {
    let client = reqwest::Client::new();
    let url = format!("https://public-api.pixiv.net/v1/works/{}.json", pid);
    let resp = client.get(&url)
        .header("Referer", "https://www.pixiv.net/")
        .send().await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Pixiv API returns {}", resp.status()));
    }

    resp.json::<PixivIllust>().await.map_err(|e| e.to_string())
}

async fn get_pixiv_ugoira_metadata(pid: i32) -> Result<PixivUgoiraMetadata, String> {
    let client = reqwest::Client::new();
    let url = format!("https://public-api.pixiv.net/v1/ugoira/{}/metadata.json", pid);
    let resp = client.get(&url)
        .header("Referer", "https://www.pixiv.net/")
        .send().await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Pixiv Ugoira API returns {}", resp.status()));
    }

    resp.json::<PixivUgoiraMetadata>().await.map_err(|e| e.to_string())
}

async fn download_pixiv_binary(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::new();
    let resp = client.get(url)
        .header("Referer", "https://www.pixiv.net/")
        .send().await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Pixiv resource download returns HTTP {}", resp.status()));
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
    let mut zip_file = archive.by_name(first_frame_name)
        .map_err(|_| {
            let alt = Path::new(first_frame_name).file_name().unwrap().to_str().unwrap();
            format!("Frame {} missing", alt)
        })?;

    let mut img_bytes = Vec::new();
    std::io::copy(&mut zip_file, &mut img_bytes).map_err(|e| e.to_string())?;
    drop(zip_file); // release archive borrow

    let img = image::load_from_memory(&img_bytes).map_err(|e| format!("Decode first frame failed: {}", e))?;
    let (width, height) = img.dimensions();

    let mut encoder = Encoder::new(&mut out_buffer, width as u16, height as u16, &[])
        .map_err(|e| e.to_string())?;
    encoder.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;

    for frame in frames {
        let name = frame.file.trim();
        let mut file = archive.by_name(name)
            .map_err(|_| {
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
