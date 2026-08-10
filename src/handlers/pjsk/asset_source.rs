use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use once_cell::sync::Lazy;

use crate::pkg::imgcache::DEFAULT_IMG_CACHE;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SekaiAssetSource {
    Snowy,
    Uni,
    Haruki,
}


const ASSET_RETRY_ROUNDS: usize = 2;

static DEFAULT_SEKAI_ASSET_SOURCES: &[SekaiAssetSource] = &[
    SekaiAssetSource::Snowy,
    SekaiAssetSource::Uni,
    SekaiAssetSource::Haruki,
];

pub static ASSET_LABEL_PATH_TEMPLATES: Lazy<HashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("event:background", vec![
        "ondemand/event/{assetbundle}/screen/bg.png",
        "ondemand/event/{assetbundle}/screen/bg.webp",
        "event/{assetbundle}/screen/bg.png",
        "event/{assetbundle}/screen/bg.webp",
    ]);
    m.insert("event:logo", vec![
        "ondemand/event/{assetbundle}/logo/logo.png",
        "ondemand/event/{assetbundle}/logo/logo.webp",
        "event/{assetbundle}/logo/logo.png",
        "event/{assetbundle}/logo/logo.webp",
    ]);
    m.insert("event:banner", vec![
        "ondemand/event_story/{assetbundle}/screen_image/banner_event_story.png",
        "ondemand/event_story/{assetbundle}/screen_image/banner_event_story.webp",
        "event_story/{assetbundle}/screen_image/banner_event_story.png",
        "event_story/{assetbundle}/screen_image/banner_event_story.webp",
        "ondemand/event/{assetbundle}/logo/logo.png",
        "ondemand/event/{assetbundle}/logo/logo.webp",
        "event/{assetbundle}/logo/logo.png",
        "event/{assetbundle}/logo/logo.webp",
        "ondemand/event/{assetbundle}/screen/bg.png",
        "ondemand/event/{assetbundle}/screen/bg.webp",
        "event/{assetbundle}/screen/bg.png",
        "event/{assetbundle}/screen/bg.webp",
        "ondemand/home/banner/{assetbundle}/{assetbundle}.png",
        "ondemand/home/banner/{assetbundle}/{assetbundle}.webp",
        "home/banner/{assetbundle}/{assetbundle}.png",
        "home/banner/{assetbundle}/{assetbundle}.webp",
    ]);
    m.insert("card:thumbnail", vec![
        "startapp/thumbnail/chara/{assetbundle}_{status}.png",
        "startapp/thumbnail/chara/{assetbundle}_{status}.webp",
        "thumbnail/chara/{assetbundle}_{status}.png",
        "thumbnail/chara/{assetbundle}_{status}.webp",
    ]);
    m.insert("card:image", vec![
        "startapp/character/member/{assetbundle}/{card_file}.png",
        "startapp/character/member/{assetbundle}/{card_file}.webp",
        "character/member/{assetbundle}/{card_file}.png",
        "character/member/{assetbundle}/{card_file}.webp",
    ]);
    m.insert("music:jacket", vec![
        "startapp/music/jacket/{assetbundle}/{assetbundle}.png",
        "startapp/music/jacket/{assetbundle}/{assetbundle}.webp",
        "music/jacket/{assetbundle}/{assetbundle}.png",
        "music/jacket/{assetbundle}/{assetbundle}.webp",
    ]);
    m
});

pub static SEKAI_ASSETS_LIST: Lazy<Vec<SekaiAssetSource>> = Lazy::new(|| {
    let raw = env::var("SEKAI_ASSET").unwrap_or_default();
    parse_sekai_asset_sources(&raw)
});

pub static LABEL_URL_CACHE: Lazy<Arc<RwLock<HashMap<String, String>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(HashMap::new()))
});

fn parse_sekai_asset_sources(raw: &str) -> Vec<SekaiAssetSource> {
    let raw = raw.trim();
    if raw.is_empty() {
        return DEFAULT_SEKAI_ASSET_SOURCES.to_vec();
    }

    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for token in raw.split(',') {
        let token = token.trim().to_lowercase();
        if token.is_empty() {
            continue;
        }

        let source = match token.as_str() {
            "snowy" | "snowyassets" => SekaiAssetSource::Snowy,
            "uni" => SekaiAssetSource::Uni,
            "haruki" => SekaiAssetSource::Haruki,
            _ => {
                tracing::warn!(
                    ?token,
                    "忽略无效 SEKAI_ASSET 配置项 (支持: snowy, uni, haruki)"
                );
                continue;
            }
        };

        if seen.insert(source) {
            sources.push(source);
        }
    }

    if sources.is_empty() {
        DEFAULT_SEKAI_ASSET_SOURCES.to_vec()
    } else {
        sources
    }
}

fn asset_base_candidates(source: SekaiAssetSource, server: &str) -> Vec<String> {
    match source {
        SekaiAssetSource::Snowy => {
            if server == "cn" {
                vec![
                    "https://snowyassets.exmeaning.com/cn".to_string(),
                    "https://snowyassets.exmeaning.com".to_string(),
                ]
            } else {
                vec!["https://snowyassets.exmeaning.com".to_string()]
            }
        }
        SekaiAssetSource::Uni => {
            vec!["https://assets.unipjsk.com".to_string()]
        }
        SekaiAssetSource::Haruki => {
            if server == "cn" {
                vec![
                    "https://sekai-assets-bdf29c81.seiunx.net/cn-assets".to_string(),
                    "https://sekai-assets-bdf29c81.seiunx.net/jp-assets".to_string(),
                ]
            } else {
                vec!["https://sekai-assets-bdf29c81.seiunx.net/jp-assets".to_string()]
            }
        }
    }
}

fn join_asset_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn build_asset_candidates(server: &str, relative_paths: &[String]) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for source in &*SEKAI_ASSETS_LIST {
        let bases = asset_base_candidates(*source, server);
        for base in bases {
            for relative_path in relative_paths {
                let url = join_asset_url(&base, relative_path);
                if seen.insert(url.clone()) {
                    urls.push(url);
                }
            }
        }
    }

    urls
}

fn label_cache_key(server: &str, label: &str) -> String {
    format!("{}:{}", server, label)
}

async fn cached_asset_url(server: &str, label: &str) -> String {
    let cache = LABEL_URL_CACHE.read().await;
    cache.get(&label_cache_key(server, label)).cloned().unwrap_or_default()
}

async fn update_cached_asset_url(server: &str, label: &str, url: &str) {
    if label.is_empty() || url.is_empty() {
        return;
    }
    let mut cache = LABEL_URL_CACHE.write().await;
    cache.insert(label_cache_key(server, label), url.to_string());
}

fn prioritize_asset_candidates(candidates: Vec<String>, preferred: &str) -> Vec<String> {
    if preferred.is_empty() {
        return candidates;
    }

    let mut ordered = Vec::with_capacity(candidates.len() + 1);
    let mut seen = std::collections::HashSet::with_capacity(candidates.len() + 1);

    ordered.push(preferred.to_string());
    seen.insert(preferred.to_string());

    for candidate in candidates {
        if seen.insert(candidate.clone()) {
            ordered.push(candidate);
        }
    }

    ordered
}

pub async fn download_asset_with_fallback(server: &str, label: &str, relative_paths: &[String]) -> String {
    let mut candidates = build_asset_candidates(server, relative_paths);
    if candidates.is_empty() {
        tracing::warn!(%label, "资源候选地址为空");
        return String::new();
    }

    let preferred = cached_asset_url(server, label).await;
    candidates = prioritize_asset_candidates(candidates, &preferred);

    for round in 0..ASSET_RETRY_ROUNDS {
        for candidate in &candidates {
            let data_url = DEFAULT_IMG_CACHE.download(candidate, None, None).await;
            if !data_url.is_empty() {
                update_cached_asset_url(server, label, candidate).await;
                return data_url;
            }
        }

        if round + 1 < ASSET_RETRY_ROUNDS {
            tokio::time::sleep(Duration::from_millis((round + 1) as u64 * 300)).await;
        }
    }

    tracing::warn!(
        %label,
        %server,
        candidates = candidates.len(),
        "资源下载失败"
    );
    String::new()
}

pub fn build_relative_paths_by_label(label: &str) -> (String, Vec<String>) {
    let parts: Vec<&str> = label.trim().splitn(4, ':').collect();
    if parts.len() < 3 {
        return (String::new(), Vec::new());
    }

    let category = parts[0].trim();
    let kind = parts[1].trim();
    let assetbundle_name = parts[2].trim();
    if category.is_empty() || kind.is_empty() || assetbundle_name.is_empty() {
        return (String::new(), Vec::new());
    }

    let label_type = format!("{}:{}", category, kind);
    let templates = match ASSET_LABEL_PATH_TEMPLATES.get(label_type.as_str()) {
        Some(t) => t,
        None => return (String::new(), Vec::new()),
    };

    let mut status = "";
    if parts.len() == 4 {
        status = parts[3].trim();
    }

    let mut status = status.to_string();
    match label_type.as_str() {
        "card:thumbnail" => {
            if status.is_empty() {
                status = "normal".to_string();
            }
        }
        "card:image" => {
            if status.starts_with("card_") {
                status = status["card_".len()..].to_string();
            }
            if status.is_empty() {
                status = "normal".to_string();
            }
        }
        _ => {}
    }

    let card_file = if label_type == "card:image" {
        format!("card_{}", status)
    } else {
        String::new()
    };

    let mut relative_paths = Vec::new();
    for tmpl in templates {
        let path = tmpl
            .replace("{assetbundle}", assetbundle_name)
            .replace("{status}", &status)
            .replace("{card_file}", &card_file);
        relative_paths.push(path);
    }

    let mut normalized_label = format!("{}:{}", label_type, assetbundle_name);
    if label_type == "card:thumbnail" || label_type == "card:image" {
        normalized_label = format!("{}:{}", normalized_label, status);
    }

    (normalized_label, relative_paths)
}

pub async fn download_asset_by_label(server: &str, label: &str) -> String {
    let (normalized_label, relative_paths) = build_relative_paths_by_label(label);
    if normalized_label.is_empty() || relative_paths.is_empty() {
        tracing::warn!(?label, "不支持的资源 label");
        return String::new();
    }
    download_asset_with_fallback(server, &normalized_label, &relative_paths).await
}
