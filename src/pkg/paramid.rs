use axum::{
    body::Body,
    http::{Request, StatusCode, Uri},
    middleware::Next,
    response::Response,
};
use redis::AsyncCommands;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use url::form_urlencoded;

pub const QUERY_PARAM_NAME: &str = "param_id";
pub const DEFAULT_KEY_TEMPLATE: &str = "amiabot-pages:params:{id}";

#[derive(Clone)]
pub struct ParamIDMiddleware {
    client: Option<redis::Client>,
    key_template: String,
}

impl ParamIDMiddleware {
    pub fn new_from_env() -> Result<Self, String> {
        let addr = env::var("VALKEY_ADDR")
            .or_else(|_| env::var("REDIS_ADDR"))
            .unwrap_or_default();
        let addr = addr.trim();
        if addr.is_empty() {
            return Ok(Self {
                client: None,
                key_template: DEFAULT_KEY_TEMPLATE.to_string(),
            });
        }

        let db = env::var("VALKEY_DB")
            .or_else(|_| env::var("REDIS_DB"))
            .unwrap_or_else(|_| "0".to_string());
        let password = env::var("VALKEY_PASSWORD")
            .or_else(|_| env::var("REDIS_PASSWORD"))
            .unwrap_or_default();

        let mut url_str = if addr.starts_with("redis://") || addr.starts_with("rediss://") {
            addr.to_string()
        } else {
            format!("redis://{}", addr)
        };

        // Inject password and DB into URL if necessary
        if !password.is_empty() {
            if let Ok(mut parsed) = url::Url::parse(&url_str) {
                let _ = parsed.set_password(Some(&password));
                url_str = parsed.to_string();
            }
        }
        if let Ok(mut parsed) = url::Url::parse(&url_str) {
            parsed.set_path(&db);
            url_str = parsed.to_string();
        }

        let client = redis::Client::open(url_str).map_err(|e| format!("Valkey client error: {}", e))?;
        let mut key_template = env::var("VALKEY_KEY_TEMPLATE")
            .unwrap_or_else(|_| DEFAULT_KEY_TEMPLATE.to_string());
        if !key_template.contains("{id}") {
            return Err(format!("VALKEY_KEY_TEMPLATE 必须包含 {{id}}: {:?}", key_template));
        }

        Ok(Self {
            client: Some(client),
            key_template,
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.client.is_some()
    }

    pub async fn handle(&self, mut req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
        let uri = req.uri().clone();
        let query_str = uri.query().unwrap_or_default();
        let query_map: HashMap<String, String> = form_urlencoded::parse(query_str.as_bytes())
            .into_owned()
            .collect();

        if let Some(param_id) = query_map.get(QUERY_PARAM_NAME) {
            let param_id = param_id.trim();
            if param_id.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }

            let client = match &self.client {
                Some(c) => c,
                None => return Ok(next.run(req).await),
            };

            let key = self.key_template.replace("{id}", param_id);
            let mut conn = match client.get_async_connection().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "param_id: Valkey 连接失败");
                    return Err(StatusCode::BAD_GATEWAY);
                }
            };

            let raw_json: Option<String> = match conn.get(&key).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error = %e, %key, "param_id: 读取 Valkey 失败");
                    return Err(StatusCode::BAD_GATEWAY);
                }
            };
            let raw_json = match raw_json {
                Some(json) => json,
                None => {
                    tracing::warn!(%key, "param_id: key 不存在或已过期");
                    return Err(StatusCode::BAD_REQUEST);
                }
            };

            let injected_map = match decode_stored_query(&raw_json) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(error = %e, %key, "param_id: 解析存储参数失败");
                    return Err(StatusCode::BAD_REQUEST);
                }
            };

            // Merge values: request values take priority over injected values
            let mut merged_query = form_urlencoded::Serializer::new(String::new());
            
            // First, write non-param_id request parameters
            for (k, v) in &query_map {
                if k != QUERY_PARAM_NAME {
                    merged_query.append_pair(k, v);
                }
            }

            // Next, write injected parameters if they are not already in request query
            for (k, values) in &injected_map {
                if !query_map.contains_key(k) {
                    for v in values {
                        merged_query.append_pair(k, v);
                    }
                }
            }

            let new_query_str = merged_query.finish();
            let mut parts = uri.into_parts();
            let path = parts.path_and_query.as_ref().map(|pq| pq.path()).unwrap_or("/");
            
            let path_and_query_str = if new_query_str.is_empty() {
                path.to_string()
            } else {
                format!("{}?{}", path, new_query_str)
            };

            if let Ok(path_and_query) = axum::http::uri::PathAndQuery::from_maybe_shared(path_and_query_str) {
                parts.path_and_query = Some(path_and_query);
                if let Ok(new_uri) = Uri::from_parts(parts) {
                    *req.uri_mut() = new_uri;
                }
            }
        }

        Ok(next.run(req).await)
    }
}

fn decode_stored_query(raw: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let val: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let obj = val.as_object().ok_or("Not a JSON object")?;

    let mut result = HashMap::new();
    for (k, v) in obj {
        let trimmed_key = k.trim().to_string();
        if trimmed_key.is_empty() {
            return Err("Empty key not allowed".to_string());
        }
        if trimmed_key == QUERY_PARAM_NAME {
            continue;
        }

        let normalized = normalize_value(v)?;
        result.insert(trimmed_key, normalized);
    }
    Ok(result)
}

fn normalize_value(v: &Value) -> Result<Vec<String>, String> {
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Bool(b) => Ok(vec![b.to_string()]),
        Value::Number(n) => Ok(vec![n.to_string()]),
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(arr) => {
            let mut list = Vec::new();
            for item in arr {
                match item {
                    Value::Null => continue,
                    Value::Bool(b) => list.push(b.to_string()),
                    Value::Number(n) => list.push(n.to_string()),
                    Value::String(s) => list.push(s.clone()),
                    Value::Array(_) | Value::Object(_) => return Err("Nested array/object not supported".to_string()),
                }
            }
            Ok(list)
        }
        Value::Object(_) => Err("Object not supported".to_string()),
    }
}
