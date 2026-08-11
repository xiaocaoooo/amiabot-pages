use crate::pkg::http_error::format_upstream_http_error;
use axum::{
    extract::{Path as AxumPath, Query},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};

use crate::handlers::pjsk::VALID_SERVERS;

const MASTER_DATA_CACHE_DIR: &str = "cache/pjsk";
const COMMIT_SHA_FILE: &str = ".commit_sha";

pub static COMMIT_SHAS: Lazy<Arc<RwLock<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub struct RefreshQuery {
    pub force: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct GhCommit {
    sha: String,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct GhContentsEntry {
    name: String,
    r#type: String,
}

fn server_to_repo_code(server: &str) -> &str {
    match server {
        "tw" => "tc",
        other => other,
    }
}

pub fn db_diff_name(server: &str) -> String {
    if server == "jp" {
        "sekai-master-db-diff".to_string()
    } else {
        format!("sekai-master-db-{}-diff", server_to_repo_code(server))
    }
}

pub fn server_cache_dir(server: &str) -> PathBuf {
    Path::new(MASTER_DATA_CACHE_DIR).join(db_diff_name(server))
}

fn remote_url(server: &str, file: &str) -> String {
    format!(
        "https://sekai-world.github.io/{}/{}",
        db_diff_name(server),
        file
    )
}

fn github_request(url: &str) -> Result<reqwest::RequestBuilder, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut builder = client
        .get(url)
        .header("User-Agent", "amiabot-pages/1.0")
        .header("Accept", "application/vnd.github.v3+json");

    if let Ok(token) = env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", token.trim()));
        }
    }
    Ok(builder)
}

async fn fetch_latest_commit_sha(server: &str) -> Result<String, String> {
    let repo = db_diff_name(server);
    let url = format!(
        "https://api.github.com/repos/Sekai-World/{}/commits?sha=main&per_page=1",
        repo
    );

    let builder = github_request(&url)?;
    let resp = crate::pkg::http_client::send(builder)
        .await
        .map_err(|e| format!("请求 GitHub API 失败 ({}): {}", server, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error(
            &format!("GitHub API ({server})"),
            status,
            &body,
        ));
    }

    let commits = resp
        .json::<Vec<GhCommit>>()
        .await
        .map_err(|e| format!("解析 commit 响应失败 ({}): {}", server, e))?;
    if commits.is_empty() {
        return Err(format!("未获取到 commit ({})", server));
    }
    Ok(commits[0].sha.clone())
}

fn load_saved_sha(server: &str) -> String {
    let path = server_cache_dir(server).join(COMMIT_SHA_FILE);
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

async fn save_sha(server: &str, sha: &str) {
    let mut shas = COMMIT_SHAS.write().await;
    shas.insert(server.to_string(), sha.to_string());
    drop(shas);

    let dir = server_cache_dir(server);
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(COMMIT_SHA_FILE), sha.as_bytes());
}

async fn fetch_file_list(server: &str) -> Result<Vec<String>, String> {
    let repo = db_diff_name(server);
    let url = format!(
        "https://api.github.com/repos/Sekai-World/{}/contents/",
        repo
    );

    let builder = github_request(&url)?;
    let resp = crate::pkg::http_client::send(builder)
        .await
        .map_err(|e| format!("请求 GitHub API 失败 ({}): {}", server, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error(
            &format!("GitHub API ({server})"),
            status,
            &body,
        ));
    }

    let entries = resp
        .json::<Vec<GhContentsEntry>>()
        .await
        .map_err(|e| format!("解析 GitHub API 响应失败 ({}): {}", server, e))?;
    let mut files = Vec::new();
    for entry in entries {
        if entry.r#type == "file" && entry.name.ends_with(".json") {
            files.push(entry.name);
        }
    }
    Ok(files)
}

async fn download_file(server: &str, file: &str) -> Result<(), String> {
    let url = remote_url(server, file);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default();

    let resp = crate::pkg::http_client::send(
        client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
    )
    .await
    .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error(
            &format!("masterdata 下载 {url}"),
            status,
            &body,
        ));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let path = server_cache_dir(server).join(file);
    let _ = fs::create_dir_all(path.parent().unwrap());
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn refresh_server(
    server: &str,
    max_concurrency: usize,
    force: bool,
) -> HashMap<String, String> {
    let mut results = HashMap::new();

    let remote_sha = match fetch_latest_commit_sha(server).await {
        Ok(sha) => sha,
        Err(e) => {
            results.insert("_error".to_string(), e);
            return results;
        }
    };

    let files = match fetch_file_list(server).await {
        Ok(f) => f,
        Err(e) => {
            results.insert("_error".to_string(), e);
            return results;
        }
    };

    if !force {
        let shas = COMMIT_SHAS.read().await;
        let local_sha = shas
            .get(server)
            .cloned()
            .unwrap_or_else(|| load_saved_sha(server));
        drop(shas);

        if local_sha == remote_sha {
            let mut missing = 0;
            let dir = server_cache_dir(server);
            for file in &files {
                if !dir.join(file).exists() {
                    missing += 1;
                }
            }
            if missing == 0 {
                results.insert(
                    "_skipped".to_string(),
                    format!(
                        "commit SHA 未变化且文件完整: {}",
                        &remote_sha[..12.min(remote_sha.len())]
                    ),
                );
                return results;
            }
            tracing::info!(
                %server,
                missing,
                "masterdata SHA 未变化但缺少文件，继续下载"
            );
        }
    }

    let sem = Arc::new(Semaphore::new(max_concurrency));
    let mut tasks = Vec::new();

    for file in files.clone() {
        let sem_clone = Arc::clone(&sem);
        let s = server.to_string();
        tasks.push(tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();
            let mut dl_err = None;
            for attempt in 0..3 {
                match download_file(&s, &file).await {
                    Ok(_) => {
                        dl_err = None;
                        break;
                    }
                    Err(e) => {
                        dl_err = Some(e);
                        tokio::time::sleep(Duration::from_secs(attempt + 1)).await;
                    }
                }
            }
            (file, dl_err)
        }));
    }

    let mut has_error = false;
    for task in tasks {
        if let Ok((file, err)) = task.await {
            if let Some(e) = err {
                results.insert(file.clone(), e);
                has_error = true;
            } else {
                results.insert(file.clone(), "ok".to_string());
            }
        }
    }

    if has_error {
        results.insert(
            "_commit".to_string(),
            "not_saved (存在下载失败的文件)".to_string(),
        );
    } else {
        save_sha(server, &remote_sha).await;
        results.insert("_commit".to_string(), remote_sha);
    }

    results
}

pub async fn refresh_all(force: bool) -> HashMap<String, HashMap<String, String>> {
    let servers = vec!["jp", "cn", "en", "tw", "kr"];
    let mut all_results = HashMap::new();
    let mut tasks = Vec::new();

    for s in servers {
        tasks.push(tokio::spawn(async move {
            let r = refresh_server(s, 20, force).await;
            (s.to_string(), r)
        }));
    }

    for task in tasks {
        if let Ok((s, r)) = task.await {
            all_results.insert(s, r);
        }
    }

    all_results
}

pub fn read_cached_json(server: &str, file: &str) -> Result<Vec<u8>, String> {
    let path = server_cache_dir(server).join(file);
    fs::read_to_string(path)
        .map(|s| s.into_bytes())
        .map_err(|_| format!("缓存文件不存在: {}/{}，请先刷新", server, file))
}

pub async fn init_master_data() {
    let servers = vec!["jp", "cn", "en", "tw", "kr"];
    let mut shas = COMMIT_SHAS.write().await;
    for s in servers {
        let sha = load_saved_sha(s);
        if !sha.is_empty() {
            shas.insert(s.to_string(), sha);
        }
    }
    drop(shas);

    tracing::info!("开始加载 masterdata（等待完成后服务才会启动）");
    let results = refresh_all(false).await;
    let (mut total, mut failed) = (0, 0);
    for (server, server_results) in results {
        for (key, status) in server_results {
            if key.starts_with('_') {
                tracing::info!(%server, %key, %status, "masterdata 状态");
                continue;
            }
            total += 1;
            if status != "ok" {
                failed += 1;
                tracing::warn!(
                    url = %remote_url(&server, &key),
                    error = %status,
                    "masterdata 文件下载失败"
                );
            }
        }
    }
    tracing::info!(total, failed, "masterdata 加载完成");
}

pub async fn master_data_handler(
    AxumPath(raw_path): AxumPath<String>,
    Query(q): Query<RefreshQuery>,
) -> impl IntoResponse {
    let raw = raw_path.trim_start_matches('/');

    if raw == "refresh" {
        let force = q.force.map(|s| s == "true").unwrap_or(false);
        let results = refresh_all(force).await;
        return (StatusCode::OK, Json(serde_json::to_value(results).unwrap())).into_response();
    }

    let parts: Vec<&str> = raw.splitn(2, '/').collect();
    if parts.len() != 2 || parts[1].is_empty() {
        return (StatusCode::BAD_REQUEST, "无效路径").into_response();
    }

    let dir_name = parts[0];
    let file = parts[1];

    let server = if dir_name == "sekai-master-db-diff" {
        "jp"
    } else if dir_name.starts_with("sekai-master-db-") && dir_name.ends_with("-diff") {
        &dir_name["sekai-master-db-".len()..dir_name.len() - "-diff".len()]
    } else {
        return (
            StatusCode::BAD_REQUEST,
            format!("无效的仓库名: {}", dir_name),
        )
            .into_response();
    };

    if !VALID_SERVERS.contains(server) {
        return (StatusCode::BAD_REQUEST, format!("无效的服务器: {}", server)).into_response();
    }

    match read_cached_json(server, file) {
        Ok(data) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            data,
        )
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}
