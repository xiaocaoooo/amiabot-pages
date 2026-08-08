use axum::{
    extract::Query,
    response::IntoResponse,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use crate::handlers::pjsk::{VALID_SERVERS, SERVER_NAMES};
use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::render_html;

#[derive(Deserialize, Debug)]
pub struct ProfileQuery {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProfileResponse {
    pub Profile: Option<PjskProfilePageData>,
    pub Error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PjskProfilePageData {
    pub ServerName: String,
    pub ServerKey: String,
    pub UserID: String,
    pub Name: String,
    pub Rank: i32,
    pub MainCard: String,
    pub FooterExtra: String,
    // Add other fields according to templates/pjsk/profile.html
}

#[derive(Deserialize, Debug)]
struct RemoteProfileResponse {
    name: Option<String>,
    rank: Option<i32>,
    // Other fields
}

pub async fn profile_handler(Query(q): Query<ProfileQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_html("pjsk/profile.html", ProfileResponse {
            Profile: None,
            Error: Some("无效的服务器参数，支持: jp, cn, en, tw, kr".to_string()),
        }).into_response();
    }

    let user_id = q.id.unwrap_or_default().trim().to_string();
    if user_id.is_empty() {
        return render_html("pjsk/profile.html", ProfileResponse {
            Profile: None,
            Error: Some("缺少玩家 ID 参数".to_string()),
        }).into_response();
    }

    let base_url = match env::var("PJSK_PROFILE_BASEURL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return render_html("pjsk/profile.html", ProfileResponse {
            Profile: None,
            Error: Some("未配置 PJSK_PROFILE_BASEURL 环境变量".to_string()),
        }).into_response(),
    };

    match fetch_remote_profile(&base_url, &server, &user_id).await {
        Ok(prof) => {
            let data = PjskProfilePageData {
                ServerName: SERVER_NAMES.get(&server).cloned().unwrap_or_else(|| server.to_uppercase()),
                ServerKey: server,
                UserID: user_id,
                Name: prof.name.unwrap_or_else(|| "未知玩家".to_string()),
                Rank: prof.rank.unwrap_or(1),
                MainCard: String::new(),
                FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
            };
            render_html("pjsk/profile.html", ProfileResponse {
                Profile: Some(data),
                Error: None,
            }).into_response()
        }
        Err(e) => render_html("pjsk/profile.html", ProfileResponse {
            Profile: None,
            Error: Some(e),
        }).into_response(),
    }
}

pub async fn profile_raw_handler(Query(q): Query<ProfileQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return (StatusCode::BAD_REQUEST, "无效服务器").into_response();
    }

    let user_id = q.id.unwrap_or_default().trim().to_string();
    if user_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "缺少玩家 ID 参数").into_response();
    }

    let base_url = match env::var("PJSK_PROFILE_BASEURL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, "未配置 PJSK_PROFILE_BASEURL 环境变量").into_response(),
    };

    match fetch_remote_profile_bytes(&base_url, &server, &user_id).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            bytes,
        ).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}

async fn fetch_remote_profile(base_url: &str, server: &str, user_id: &str) -> Result<RemoteProfileResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = format!("{}/user/{}/profile?server={}", base_url.trim_end_matches('/'), user_id, server);
    let mut builder = client.get(&url);

    if let Ok(headers_str) = env::var("PJSK_PROFILE_HEADERS") {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&headers_str) {
            for (k, v) in map {
                builder = builder.header(k, v);
            }
        }
    }

    let resp = builder.send().await.map_err(|e| format!("请求 profile 失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("上游返回 HTTP {}", resp.status()));
    }

    resp.json::<RemoteProfileResponse>().await.map_err(|e| format!("解析 profile JSON 失败: {}", e))
}

async fn fetch_remote_profile_bytes(base_url: &str, server: &str, user_id: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = format!("{}/user/{}/profile?server={}", base_url.trim_end_matches('/'), user_id, server);
    let mut builder = client.get(&url);

    if let Ok(headers_str) = env::var("PJSK_PROFILE_HEADERS") {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&headers_str) {
            for (k, v) in map {
                builder = builder.header(k, v);
            }
        }
    }

    let resp = builder.send().await.map_err(|e| format!("请求 profile 失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("上游返回 HTTP {}", resp.status()));
    }

    resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
}
use axum::http::header;
