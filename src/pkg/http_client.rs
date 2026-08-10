use once_cell::sync::Lazy;
use reqwest::{Client, Method, Request, RequestBuilder, Response, Url};
use std::time::{Duration, Instant};

static SHARED_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| Client::new())
});

fn latency_ms(started: Instant) -> String {
    format!("{:.2}", started.elapsed().as_secs_f64() * 1000.0)
}

fn request_meta(builder: &RequestBuilder) -> (Method, String) {
    match builder.try_clone().and_then(|b| b.build().ok()) {
        Some(req) => (req.method().clone(), req.url().to_string()),
        None => (Method::GET, "<unknown>".to_string()),
    }
}

/// 发送 RequestBuilder，并在 debug 级别记录 method/url/status/latency。
/// 保留原 builder 关联 Client 的 timeout 等配置。
pub async fn send(builder: RequestBuilder) -> Result<Response, reqwest::Error> {
    let (method, url) = request_meta(&builder);
    let started = Instant::now();
    tracing::debug!(%method, %url, "http client request start");

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let latency_ms = latency_ms(started);
            tracing::debug!(%method, %url, status, latency_ms, "http client request done");
            Ok(resp)
        }
        Err(e) => {
            let latency_ms = latency_ms(started);
            tracing::debug!(%method, %url, error = %e, latency_ms, "http client request failed");
            Err(e)
        }
    }
}

/// 执行已构建的 Request（用于 gallery 等先 build 再发的路径）。
pub async fn execute(request: Request) -> Result<Response, reqwest::Error> {
    let method = request.method().clone();
    let url: Url = request.url().clone();
    let started = Instant::now();
    tracing::debug!(%method, %url, "http client request start");

    match SHARED_CLIENT.execute(request).await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let latency_ms = latency_ms(started);
            tracing::debug!(%method, %url, status, latency_ms, "http client request done");
            Ok(resp)
        }
        Err(e) => {
            let latency_ms = latency_ms(started);
            tracing::debug!(%method, %url, error = %e, latency_ms, "http client request failed");
            Err(e)
        }
    }
}
