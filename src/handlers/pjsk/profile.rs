use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Duration;

use crate::handlers::pjsk::{SERVER_NAMES, VALID_SERVERS};
use crate::handlers::{format_upstream_http_error, render_html};

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
}

#[derive(Deserialize, Debug)]
struct RemoteProfileUser {
    name: Option<String>,
    rank: Option<i32>,
}

#[derive(Deserialize, Debug)]
struct RemoteProfileResponse {
    user: Option<RemoteProfileUser>,
}

pub async fn profile_handler(Query(q): Query<ProfileQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_html(
            "pjsk/profile.html",
            ProfileResponse {
                Profile: None,
                Error: Some("无效的服务器参数，支持: jp, cn, en, tw, kr".to_string()),
            },
        )
        .into_response();
    }

    let user_id = q.id.unwrap_or_default().trim().to_string();
    if user_id.is_empty() {
        return render_html(
            "pjsk/profile.html",
            ProfileResponse {
                Profile: None,
                Error: Some("缺少玩家 ID 参数".to_string()),
            },
        )
        .into_response();
    }

    let base_url = match env::var("PJSK_PROFILE_BASEURL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            return render_html(
                "pjsk/profile.html",
                ProfileResponse {
                    Profile: None,
                    Error: Some("未配置 PJSK_PROFILE_BASEURL 环境变量".to_string()),
                },
            )
            .into_response();
        }
    };

    match fetch_remote_profile(&base_url, &server, &user_id).await {
        Ok(prof) => {
            let user = prof.user.unwrap_or(RemoteProfileUser {
                name: None,
                rank: None,
            });
            let data = PjskProfilePageData {
                ServerName: SERVER_NAMES
                    .get(&server)
                    .cloned()
                    .unwrap_or_else(|| server.to_uppercase()),
                ServerKey: server,
                UserID: user_id,
                Name: user
                    .name
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "未知玩家".to_string()),
                Rank: user.rank.unwrap_or(1),
                MainCard: String::new(),
                FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />"
                    .to_string(),
            };
            render_html(
                "pjsk/profile.html",
                ProfileResponse {
                    Profile: Some(data),
                    Error: None,
                },
            )
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, server = %server, user_id = %user_id, "profile 页面获取失败");
            render_html(
                "pjsk/profile.html",
                ProfileResponse {
                    Profile: None,
                    Error: Some(e),
                },
            )
            .into_response()
        }
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
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "未配置 PJSK_PROFILE_BASEURL 环境变量",
            )
                .into_response();
        }
    };

    match fetch_remote_profile_bytes(&base_url, &server, &user_id).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, server = %server, user_id = %user_id, "profile raw 获取失败");
            (StatusCode::BAD_GATEWAY, e).into_response()
        }
    }
}

fn profile_api_url(base_url: &str, server: &str, user_id: &str) -> String {
    format!(
        "{}/api/{}/{}/profile",
        base_url.trim_end_matches('/'),
        server,
        user_id
    )
}

fn apply_profile_headers(mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder = builder
        .header("User-Agent", "amiabot-pages/1.0")
        .header("Accept", "application/json");

    if let Ok(headers_str) = env::var("PJSK_PROFILE_HEADERS") {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&headers_str) {
            for (k, v) in map {
                builder = builder.header(k, v);
            }
        }
    }
    builder
}

async fn fetch_remote_profile(
    base_url: &str,
    server: &str,
    user_id: &str,
) -> Result<RemoteProfileResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = profile_api_url(base_url, server, user_id);
    let builder = apply_profile_headers(client.get(&url));

    let resp = builder
        .send()
        .await
        .map_err(|e| format!("请求 profile 失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error("上游 profile", status, body));
    }

    resp.json::<RemoteProfileResponse>()
        .await
        .map_err(|e| format!("解析 profile JSON 失败: {}", e))
}

async fn fetch_remote_profile_bytes(
    base_url: &str,
    server: &str,
    user_id: &str,
) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = profile_api_url(base_url, server, user_id);
    let builder = apply_profile_headers(client.get(&url));

    let resp = builder
        .send()
        .await
        .map_err(|e| format!("请求 profile 失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error("上游 profile", status, body));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

/// B30 等模块在 suite 未提供昵称时，回退到 Moe-Sekai profile 取 user.name。
pub async fn fetch_player_name(server: &str, user_id: &str) -> Option<String> {
    let base_url = env::var("PJSK_PROFILE_BASEURL").ok()?;
    if base_url.trim().is_empty() {
        return None;
    }
    match fetch_remote_profile(&base_url, server, user_id).await {
        Ok(prof) => prof
            .user
            .and_then(|u| u.name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        Err(e) => {
            tracing::warn!(error = %e, server = %server, user_id = %user_id, "回退获取玩家昵称失败");
            None
        }
    }
}
