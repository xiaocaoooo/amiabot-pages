use crate::handlers::pjsk::asset_source::{build_relative_paths_by_label, download_asset_by_label};
use crate::handlers::pjsk::VALID_SERVERS;
use axum::{
    extract::{Path as AxumPath, Query},
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

const ASSET_BINARY_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

#[derive(Deserialize, Debug)]
pub struct AssetBinaryQuery {
    pub server: Option<String>,
}

pub fn decode_data_url(data_url: &str) -> Result<(String, Vec<u8>), String> {
    let raw = data_url.trim();
    if !raw.starts_with("data:") {
        return Err("invalid data URL prefix".to_string());
    }

    let meta_and_payload = &raw["data:".len()..];
    let (meta, payload) = meta_and_payload
        .split_once(',')
        .ok_or_else(|| "invalid data URL payload".to_string())?;

    if !meta.ends_with(";base64") {
        return Err("unsupported data URL encoding".to_string());
    }

    let content_type = &meta[..meta.len() - ";base64".len()];
    let content_type = if content_type.is_empty() {
        "application/octet-stream"
    } else {
        content_type
    };

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("decode base64 failed: {}", e))?;

    Ok((content_type.to_string(), data))
}

pub async fn asset_binary_handler(
    AxumPath(label): AxumPath<String>,
    Query(q): Query<AssetBinaryQuery>,
) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return (
            StatusCode::BAD_REQUEST,
            "无效的服务器参数，支持: jp, cn, en, tw, kr",
        )
            .into_response();
    }

    let label = label.trim();
    if label.is_empty() {
        return (StatusCode::BAD_REQUEST, "缺少资源 label").into_response();
    }

    let (normalized, _) = build_relative_paths_by_label(label);
    if normalized.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            format!("不支持的资源 label: {}", label),
        )
            .into_response();
    }

    let data_url = download_asset_by_label(&server, label).await;
    if data_url.is_empty() {
        return (StatusCode::NOT_FOUND, format!("资源下载失败: {}", label)).into_response();
    }

    match decode_data_url(&data_url) {
        Ok((content_type, payload)) => (
            StatusCode::OK,
            [
                (header::CACHE_CONTROL, ASSET_BINARY_CACHE_CONTROL),
                (header::CONTENT_TYPE, content_type.as_str()),
            ],
            payload,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("解析资源失败: {}", e),
        )
            .into_response(),
    }
}
