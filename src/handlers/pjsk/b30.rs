use axum::{extract::Query, response::IntoResponse};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::pjsk::assets::read_cached_json;
use crate::handlers::pjsk::{SERVER_NAMES, VALID_SERVERS};
use crate::handlers::{format_upstream_http_error, render_html};
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;

const B30_CHART_URL: &str =
    "https://raw.githubusercontent.com/moe-sekai/MoeSekai-Hub/main/data/pjskb30/merged_chart.csv";
const PJSK_B30_AP_ICON_URL: &str =
    "https://raw.githubusercontent.com/watagashi-uni/Unibot/refs/heads/main/pics/AllPerfect.png";
const PJSK_B30_FC_ICON_URL: &str =
    "https://raw.githubusercontent.com/watagashi-uni/Unibot/refs/heads/main/pics/FullCombo.png";

#[derive(Deserialize, Debug)]
pub struct B30Query {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct B30ScoreEntry {
    pub Order: usize,
    pub Cover: String,
    pub Name: String,
    pub SongID: String,
    pub Diff: String,
    pub Level: String,
    pub ResultType: String,
    pub ResultIcon: String,
    pub Constant: String,
    pub DiffStyle: String,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct B30PageData {
    pub Name: String,
    pub Server: String,
    pub ServerKey: String,
    pub UserRating: String,
    pub Count: usize,
    pub Scores: Vec<B30ScoreEntry>,
    pub ChartUpdatedAt: String,
    pub UpdatedTime: String,
    pub FooterExtra: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct B30Response {
    pub B30: Option<B30PageData>,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct MusicEntry {
    id: i32,
    title: String,
    assetbundleName: String,
}

struct ChartCache {
    data: String,
    expires: std::time::SystemTime,
}

static B30_CHART_CACHE: Lazy<Arc<RwLock<Option<ChartCache>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct SuiteUserMusicResult {
    musicId: i32,
    musicDifficultyType: String,
    highScore: i64,
    playResult: String,
    fullComboFlg: bool,
    fullPerfectFlg: bool,
    playType: String,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct SuiteUserProfile {
    name: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct SuiteUserMusicResponse {
    userMusicResults: Vec<SuiteUserMusicResult>,
    #[serde(default)]
    userProfile: Option<SuiteUserProfile>,
    #[serde(default)]
    upload_time: Option<i64>,
}

fn diff_name_to_num(diff: &str) -> i32 {
    match diff.to_lowercase().as_str() {
        "easy" => 0,
        "normal" => 1,
        "hard" => 2,
        "expert" => 3,
        "master" => 4,
        "append" => 5,
        _ => -1,
    }
}

fn diff_num_to_label(n: i32) -> &'static str {
    match n {
        0 => "EZ",
        1 => "NM",
        2 => "HD",
        3 => "EX",
        4 => "MA",
        5 => "APD",
        _ => "?",
    }
}

fn diff_num_to_bg_style(n: i32) -> &'static str {
    match n {
        0 => "background:#5AC06E;color:#fff;",
        1 => "background:#56A4D4;color:#fff;",
        2 => "background:#EFAF28;color:#fff;",
        3 => "background:#E84D53;color:#fff;",
        4 => "background:#BB58B8;color:#fff;",
        5 => "background:#EE92BC;color:#fff;",
        _ => "background:#888;color:#fff;",
    }
}

async fn get_b30_chart_csv() -> Result<String, String> {
    {
        let cache = B30_CHART_CACHE.read().await;
        if let Some(ref c) = *cache {
            if c.expires > std::time::SystemTime::now() {
                return Ok(c.data.clone());
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let resp = crate::pkg::http_client::send(client.get(B30_CHART_URL))
        .await
        .map_err(|e| format!("获取难度表失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error("B30 难度表", status, &body));
    }

    let data = resp
        .text()
        .await
        .map_err(|e| format!("读取难度表失败: {}", e))?;

    let mut cache = B30_CHART_CACHE.write().await;
    *cache = Some(ChartCache {
        data: data.clone(),
        expires: std::time::SystemTime::now() + Duration::from_secs(3600),
    });

    Ok(data)
}

struct ChartEntry {
    level: f64,
    constant: f64,
}

fn parse_b30_chart(csv: &str) -> HashMap<i32, ChartEntry> {
    let mut result = HashMap::new();
    let lines = csv.split('\n');
    for (i, line) in lines.enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.splitn(8, ',').collect();
        if fields.len() < 7 {
            continue;
        }

        let constant: f64 = match fields[2].trim().parse() {
            Ok(c) if c > 0.0 => c,
            _ => continue,
        };
        let song_id: i32 = match fields[6].trim().parse() {
            Ok(id) if id > 0 => id,
            _ => continue,
        };

        let diff = fields[5].trim();
        let diff_num = diff_name_to_num(diff);
        if diff_num < 0 {
            continue;
        }

        let level: f64 = fields[1].trim().parse().unwrap_or(0.0);

        result.insert(song_id * 10 + diff_num, ChartEntry { level, constant });
    }
    result
}

fn result_type_rank(play_result: &str, fc: bool, ap: bool) -> i32 {
    let lower = play_result.to_lowercase();
    if lower == "not_clear" {
        return 0;
    }
    if ap {
        return 3;
    }
    if fc {
        return 2;
    }
    1 // clear
}

fn calc_rating_for_result(rt: i32, constant: f64, _level: f64) -> f64 {
    if constant <= 0.0 {
        return 0.0;
    }
    match rt {
        3 => constant + 0.3, // AP
        2 => constant + 0.2, // FC
        1 => constant,       // Clear
        _ => 0.0,
    }
}

async fn fetch_suite_music_results(
    base_url: &str,
    server: &str,
    user_id: &str,
) -> Result<(Vec<SuiteUserMusicResult>, String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    // Haruki suite-api: /public/{server}/suite/{user_id}
    let url = format!(
        "{}/public/{}/suite/{}",
        base_url.trim_end_matches('/'),
        server,
        user_id
    );

    let mut builder = client
        .get(&url)
        .header("User-Agent", "amiabot-pages/1.0")
        .header("Accept", "application/json");
    if let Ok(headers_str) = env::var("PJSK_PROFILE_HEADERS") {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&headers_str) {
            for (k, v) in map {
                builder = builder.header(k, v);
            }
        }
    }

    let resp = crate::pkg::http_client::send(builder)
        .await
        .map_err(|e| format!("请求 suite-api 失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_upstream_http_error("B30 suite-api", status, &body));
    }

    let data = resp
        .json::<SuiteUserMusicResponse>()
        .await
        .map_err(|e| format!("解析 suite-api 成绩 JSON 失败: {}", e))?;

    if data.userMusicResults.is_empty() {
        return Err("suite-api 未返回 userMusicResults 数据".to_string());
    }

    let mut name = data
        .userProfile
        .and_then(|p| p.name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    if name.is_empty() {
        name = crate::handlers::pjsk::profile::fetch_player_name(server, user_id)
            .await
            .unwrap_or_default();
    }
    if name.is_empty() {
        name = "玩家".to_string();
    }

    let upload_time = data
        .upload_time
        .map(|ts| {
            let secs = if ts < 100_000_000_000 { ts } else { ts / 1000 };
            crate::pkg::timefmt::format_unix_secs(secs)
        })
        .unwrap_or_default();

    Ok((data.userMusicResults, name, upload_time))
}

fn build_music_index(server: &str) -> HashMap<i32, MusicEntry> {
    let mut index = HashMap::new();
    if let Ok(data) = read_cached_json(server, "musics.json") {
        if let Ok(list) = serde_json::from_slice::<Vec<MusicEntry>>(&data) {
            for entry in list {
                index.insert(entry.id, entry);
            }
        }
    }
    index
}

fn render_b30_err(msg: &str) -> impl IntoResponse {
    tracing::warn!(error = %msg, "b30 页面错误");
    render_html(
        "pjsk/b30.html",
        B30Response {
            B30: None,
            Error: Some(msg.to_string()),
        },
    )
}

struct BestEntry {
    song_id: i32,
    diff_num: i32,
    diff_label: &'static str,
    level: f64,
    constant: f64,
    result_type: i32,
    result_str: &'static str,
}

pub async fn b30_handler(Query(q): Query<B30Query>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_b30_err("无效的服务器参数，支持: jp, cn, en, tw, kr").into_response();
    }

    let user_id = q.id.unwrap_or_default().trim().to_string();
    if user_id.is_empty() {
        return render_b30_err("缺少玩家 ID 参数").into_response();
    }
    if !user_id.chars().all(|c| c.is_ascii_digit()) {
        return render_b30_err("无效的玩家 ID").into_response();
    }

    let suite_base_url =
        match env::var("PJSK_SUITE_BASEURL").or_else(|_| env::var("PJSK_PROFILE_BASEURL")) {
            Ok(url) if !url.trim().is_empty() => url,
            _ => return render_b30_err("未配置 PJSK_SUITE_BASEURL 环境变量").into_response(),
        };

    // 1. 获取难度表
    let chart_csv = match get_b30_chart_csv().await {
        Ok(csv) => csv,
        Err(e) => return render_b30_err(&format!("加载难度表失败: {}", e)).into_response(),
    };
    let chart_map = parse_b30_chart(&chart_csv);

    // 2. 获取音乐成绩
    let (music_results, name, upload_time) =
        match fetch_suite_music_results(&suite_base_url, &server, &user_id).await {
            Ok(res) => res,
            Err(e) => return render_b30_err(&e).into_response(),
        };

    // 3. 并发合并最佳成绩
    let mut best_map: HashMap<i32, BestEntry> = HashMap::new();
    for r in music_results {
        let song_id = r.musicId;
        let diff_num = diff_name_to_num(&r.musicDifficultyType);
        if song_id <= 0 || diff_num < 0 {
            continue;
        }

        let chart = match chart_map.get(&(song_id * 10 + diff_num)) {
            Some(c) => c,
            None => continue,
        };

        let rt = result_type_rank(&r.playResult, r.fullComboFlg, r.fullPerfectFlg);
        if rt == 0 {
            continue;
        }

        let key = song_id * 10 + diff_num;
        let replace = match best_map.get(&key) {
            Some(existing) => rt > existing.result_type,
            None => true,
        };

        if replace {
            let result_str = match rt {
                3 => "ap",
                2 => "fc",
                _ => "clear",
            };
            best_map.insert(
                key,
                BestEntry {
                    song_id,
                    diff_num,
                    diff_label: diff_num_to_label(diff_num),
                    level: chart.level,
                    constant: chart.constant,
                    result_type: rt,
                    result_str,
                },
            );
        }
    }

    // 4. 计算 Rating 并排序
    struct ScoredEntry {
        entry: BestEntry,
        rating: f64,
    }

    let mut scored = Vec::new();
    for (_, entry) in best_map {
        let rating = calc_rating_for_result(entry.result_type, entry.constant, entry.level);
        if rating <= 0.0 {
            continue;
        }
        scored.push(ScoredEntry { entry, rating });
    }

    scored.sort_by(|a, b| {
        b.rating
            .partial_cmp(&a.rating)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if scored.len() > 30 {
        scored.truncate(30);
    }

    let sum_rating: f64 = scored.iter().map(|s| s.rating).sum();
    let server_name = SERVER_NAMES
        .get(&server)
        .cloned()
        .unwrap_or_else(|| server.to_uppercase());
    let music_idx = build_music_index(&server);

    // 5. 转换为模板数据并异步拉取封面与结果图片
    let mut views = Vec::new();
    for (i, s) in scored.into_iter().enumerate() {
        let result_icon = match s.entry.result_str {
            "ap" => {
                DEFAULT_IMG_CACHE
                    .download(PJSK_B30_AP_ICON_URL, None, None)
                    .await
            }
            "fc" => {
                DEFAULT_IMG_CACHE
                    .download(PJSK_B30_FC_ICON_URL, None, None)
                    .await
            }
            _ => String::new(),
        };

        let constant_text = format!("{:.1}", s.entry.constant);
        let mut song_name = String::new();
        let mut cover = String::new();

        if let Some(m) = music_idx.get(&s.entry.song_id) {
            song_name = m.title.clone();
            let label = format!("music:jacket:{}", m.assetbundleName);
            cover = download_asset_by_label(&server, &label).await;
        }

        views.push(B30ScoreEntry {
            Order: i + 1,
            Cover: cover,
            Name: truncate_str(&song_name, 20),
            SongID: format!("#{}", s.entry.song_id),
            Diff: s.entry.diff_label.to_string(),
            Level: format!("{:.0}", s.entry.level),
            ResultType: s.entry.result_str.to_string(),
            ResultIcon: result_icon,
            Constant: constant_text,
            DiffStyle: diff_num_to_bg_style(s.entry.diff_num).to_string(),
        });
    }

    let rating_val = if views.is_empty() {
        0.0
    } else {
        sum_rating / 30.0
    };
    let page = B30PageData {
        Name: name,
        Server: server_name,
        ServerKey: server,
        UserRating: format!("{:.2}", rating_val),
        Count: views.len(),
        Scores: views,
        ChartUpdatedAt: String::new(), // Placeholder as Go version
        UpdatedTime: upload_time,
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
    };

    render_html(
        "pjsk/b30.html",
        B30Response {
            B30: Some(page),
            Error: None,
        },
    )
    .into_response()
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let mut chars = s.chars();
    let mut res = String::new();
    for _ in 0..max_len {
        if let Some(c) = chars.next() {
            res.push(c);
        } else {
            break;
        }
    }
    if chars.next().is_some() {
        res.push('…');
    }
    res
}
