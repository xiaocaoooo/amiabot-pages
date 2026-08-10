use axum::{
    extract::Query,
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;
use crate::handlers::render_html;

const ZEABUR_GRAPHQL_ENDPOINT: &str = "https://api.zeabur.com/graphql";

const ZEABUR_STATUS_QUERY: &str = r#"
query Query {
  servers {
    _id
    city
    country
    name
    status {
      isOnline
      totalCPU
      usedCPU
      totalMemory
      usedMemory
      totalDisk
      usedDisk
      latency
    }
  }
  projects {
    edges {
      node {
        _id
        iconURL
        name
        services {
          _id
          name
          status
        }
      }
    }
  }
}
"#;

#[derive(Deserialize, Debug)]
pub struct ZeaburGraphQLRequest {
    pub query: &'static str,
}

#[derive(Deserialize, Debug)]
struct ZeaburGraphQLResponse {
    data: Option<ZeaburStatusData>,
    errors: Option<Vec<ZeaburGraphQLErr>>,
}

#[derive(Deserialize, Debug)]
struct ZeaburGraphQLErr {
    message: String,
}

#[derive(Deserialize, Debug)]
struct ZeaburStatusData {
    servers: Vec<ZeaburServer>,
    projects: ZeaburProjectConnection,
}

#[derive(Deserialize, Debug)]
struct ZeaburServer {
    _id: String,
    city: String,
    country: String,
    name: String,
    status: ZeaburServerState,
}

#[derive(Deserialize, Debug)]
struct ZeaburServerState {
    isOnline: bool,
    totalCPU: f64,
    usedCPU: f64,
    totalMemory: f64,
    usedMemory: f64,
    totalDisk: f64,
    usedDisk: f64,
    latency: f64,
}

#[derive(Deserialize, Debug)]
struct ZeaburProjectConnection {
    edges: Vec<ZeaburProjectEdge>,
}

#[derive(Deserialize, Debug)]
struct ZeaburProjectEdge {
    node: ZeaburProject,
}

#[derive(Deserialize, Debug)]
struct ZeaburProject {
    _id: String,
    iconURL: String,
    name: String,
    services: Vec<ZeaburService>,
}

#[derive(Deserialize, Debug)]
struct ZeaburService {
    _id: String,
    name: String,
    status: String,
}

#[derive(Serialize, Clone)]
pub struct ZeaburServerView {
    pub Name: String,
    pub Location: String,
    pub Online: bool,
    pub Latency: String,
    pub CPU: String,
    pub Memory: String,
    pub Disk: String,
}

#[derive(Serialize, Clone)]
pub struct ZeaburServiceView {
    pub Name: String,
    pub Status: String,
}

#[derive(Serialize, Clone)]
pub struct ZeaburProjectView {
    pub Name: String,
    pub IconURL: String,
    pub ServiceCount: usize,
    pub RunningCount: usize,
    pub Services: Vec<ZeaburServiceView>,
}

#[derive(Serialize, Clone)]
pub struct ZeaburStatusPageData {
    pub FetchedAt: String,
    pub ServerCount: usize,
    pub OnlineCount: usize,
    pub ProjectCount: usize,
    pub ServiceCount: usize,
    pub Servers: Vec<ZeaburServerView>,
    pub Projects: Vec<ZeaburProjectView>,
}

#[derive(Serialize)]
pub struct ZeaburResponse {
    pub Status: Option<ZeaburStatusPageData>,
    pub Error: Option<String>,
}

fn resolve_zeabur_token(headers: &HeaderMap) -> String {
    if let Some(auth_header) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        let auth_header = auth_header.trim();
        if auth_header.to_lowercase().starts_with("bearer ") {
            return auth_header["bearer ".len()..].trim().to_string();
        }
        return auth_header.to_string();
    }
    env::var("ZEABUR_TOKEN").unwrap_or_default().trim().to_string()
}

fn format_ratio(used: f64, total: f64) -> String {
    if total <= 0.0 {
        return format!("{:.2} / {:.2}", used, total);
    }
    let percent = used / total * 100.0;
    format!("{:.2} / {:.2} ({:.1}%)", used, total, percent)
}

fn format_latency(v: f64) -> String {
    if v <= 0.0 || v.is_nan() || v.is_infinite() {
        return "-".to_string();
    }
    format!("{:.0}ms", v)
}

async fn build_zeabur_status_page(data: ZeaburStatusData) -> ZeaburStatusPageData {
    let mut page = ZeaburStatusPageData {
        FetchedAt: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ServerCount: data.servers.len(),
        OnlineCount: 0,
        ProjectCount: data.projects.edges.len(),
        ServiceCount: 0,
        Servers: Vec::new(),
        Projects: Vec::new(),
    };

    for s in data.servers {
        if s.status.isOnline {
            page.OnlineCount += 1;
        }

        let mut loc = format!("{} {}", s.country, s.city);
        loc = loc.trim().to_string();
        if loc.is_empty() {
            loc = "-".to_string();
        }

        page.Servers.push(ZeaburServerView {
            Name: s.name,
            Location: loc,
            Online: s.status.isOnline,
            Latency: format_latency(s.status.latency),
            CPU: format_ratio(s.status.usedCPU, s.status.totalCPU),
            Memory: format_ratio(s.status.usedMemory, s.status.totalMemory),
            Disk: format_ratio(s.status.usedDisk, s.status.totalDisk),
        });
    }

    for edge in data.projects.edges {
        let node = edge.node;
        let icon_url = DEFAULT_IMG_CACHE.download(&node.iconURL, None, None).await;

        let mut proj_view = ZeaburProjectView {
            Name: node.name,
            IconURL: icon_url,
            ServiceCount: node.services.len(),
            RunningCount: 0,
            Services: Vec::new(),
        };

        for svc in node.services {
            let status = svc.status.trim().to_uppercase();
            let status = if status.is_empty() { "UNKNOWN".to_string() } else { status };
            if status == "RUNNING" {
                proj_view.RunningCount += 1;
            }
            proj_view.Services.push(ZeaburServiceView {
                Name: svc.name,
                Status: status,
            });
        }

        page.ServiceCount += proj_view.ServiceCount;
        page.Projects.push(proj_view);
    }

    page
}

fn render_zeabur_error(msg: &str) -> impl IntoResponse {
    render_html("status/zeabur.html", ZeaburResponse {
        Status: None,
        Error: Some(msg.to_string()),
    })
}

pub async fn zeabur_page_handler(headers: HeaderMap) -> impl IntoResponse {
    let token = resolve_zeabur_token(&headers);
    if token.is_empty() {
        return render_zeabur_error("缺少 Zeabur token，请在环境变量 ZEABUR_TOKEN 配置，或在请求头传 Authorization: Bearer <token>").into_response();
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let request_payload = serde_json::json!({ "query": ZEABUR_STATUS_QUERY });

    let resp = match crate::pkg::http_client::send(
        client.post(ZEABUR_GRAPHQL_ENDPOINT)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("X-Request-Type", "GraphQL")
            .json(&request_payload)
    ).await
    {
        Ok(r) => r,
        Err(e) => return render_zeabur_error(&format!("请求 Zeabur 失败: {}", e)).into_response(),
    };

    let status_code = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if !status_code.is_success() {
        return render_zeabur_error(&format!("Zeabur API 返回异常状态码: {}\n{}", status_code, body_text.trim())).into_response();
    }

    let payload: ZeaburGraphQLResponse = match serde_json::from_str(&body_text) {
        Ok(p) => p,
        Err(e) => return render_zeabur_error(&format!("解析 Zeabur 响应失败: {}\nResponse: {}", e, body_text)).into_response(),
    };

    if let Some(errs) = payload.errors {
        if !errs.is_empty() && payload.data.is_none() {
            return render_zeabur_error(&format!("Zeabur GraphQL 返回错误: {}", errs[0].message)).into_response();
        }
    }

    let data = match payload.data {
        Some(d) => d,
        None => return render_zeabur_error("Zeabur 数据为空").into_response(),
    };

    let page_data = build_zeabur_status_page(data).await;
    render_html("status/zeabur.html", ZeaburResponse {
        Status: Some(page_data),
        Error: None,
    }).into_response()
}
