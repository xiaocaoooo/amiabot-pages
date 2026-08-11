use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use regex::Regex;
use once_cell::sync::Lazy;

use crate::handlers::pjsk::{SERVER_NAMES, VALID_SERVERS};
use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::pjsk::card::{star_positions, has_special_training, rarity_name, attr_name, unit_name};
use crate::handlers::pjsk::assets::read_cached_json;
use crate::handlers::{format_upstream_http_error, render_html};

static PJSK_PROFILE_WORD_PLACEHOLDER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<#.*?>").unwrap()
});

static PJSK_PROFILE_CHARACTER_GRID_ORDER: &[i32] = &[
    21, 22, 23, 24, 25, 26,
    1, 2, 3, 4, 0, 0,
    5, 6, 7, 8, 0, 0,
    9, 10, 11, 12, 0, 0,
    13, 14, 15, 16, 0, 0,
    17, 18, 19, 20, 0, 0,
];

static PJSK_PROFILE_RADAR_ORDER: &[i32] = &[
    21, 22, 23, 24, 25, 26, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20
];

static PJSK_PROFILE_UNIT_COLORS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("light_sound", "#4455DD");
    m.insert("idol", "#88DD44");
    m.insert("street", "#EE1166");
    m.insert("theme_park", "#FF9900");
    m.insert("school_refusal", "#884499");
    m.insert("piapro", "#33CCBB");
    m
});

static PJSK_PROFILE_CHARACTER_COLORS: Lazy<HashMap<i32, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(1, "#33aaee"); m.insert(2, "#ffdd44"); m.insert(3, "#ee6666"); m.insert(4, "#bbdd22");
    m.insert(5, "#ffccaa"); m.insert(6, "#99ccff"); m.insert(7, "#ffaacc"); m.insert(8, "#99eedd");
    m.insert(9, "#ff6699"); m.insert(10, "#00bbdd"); m.insert(11, "#ff7722"); m.insert(12, "#0077dd");
    m.insert(13, "#ffbb00"); m.insert(14, "#ff66bb"); m.insert(15, "#33dd99"); m.insert(16, "#bb88ee");
    m.insert(17, "#bb6688"); m.insert(18, "#8888cc"); m.insert(19, "#ccaa88"); m.insert(20, "#ddaacc");
    m.insert(21, "#33ccbb"); m.insert(22, "#ffcc11"); m.insert(23, "#ffee11"); m.insert(24, "#ffbbcc");
    m.insert(25, "#dd4444"); m.insert(26, "#3366cc");
    m
});

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct ProfileQuery {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct ProfileResponse {
    pub Profile: Option<PjskProfilePageData>,
    pub Error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfileCardView {
    pub ID: i32,
    pub Prefix: String,
    pub CharacterName: String,
    pub CharacterUnit: String,
    pub Rarity: String,
    pub Attr: String,
    pub Level: i32,
    pub MasterRank: i32,
    pub ImageMode: String,
    pub Thumbnail: String,
    pub Frame: String,
    pub AttrIcon: String,
    pub Stars: Vec<i32>,
    pub StarIcon: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfileDifficultyColumn {
    pub Label: String,
    pub BackgroundStyle: String,
    pub CellStyle: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfilePlayStatsCell {
    pub Value: i32,
    pub Style: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfilePlayStatsRow {
    pub Label: String,
    pub Values: Vec<PjskProfilePlayStatsCell>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfileHonorView {
    pub Slot: String,
    pub Title: String,
    pub Subtitle: String,
    pub Level: i32,
    pub Kind: String,
    pub Rarity: String,
    pub Description: String,
    pub IsMain: bool,
    pub HasArtwork: bool,
    pub Artwork: String,
    pub Width: i32,
    pub Height: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfileCharacterRankView {
    pub Name: String,
    pub Rank: i32,
    pub Empty: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfileChallengeLiveView {
    pub Available: bool,
    pub CharacterName: String,
    pub StageRank: i32,
    pub HighScore: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct PjskProfilePageData {
    pub ServerName: String,
    pub ServerKey: String,
    pub UserID: String,
    pub Name: String,
    pub Rank: i32,
    pub Server: String,
    pub Word: String,
    pub TwitterID: String,
    pub UpdatedAt: String,
    pub DeckCards: Vec<PjskProfileCardView>,
    pub HasLeaderCard: bool,
    pub LeaderCard: Option<PjskProfileCardView>,
    pub Honors: Vec<PjskProfileHonorView>,
    pub HasHonors: bool,
    pub DifficultyColumns: Vec<PjskProfileDifficultyColumn>,
    pub PlayStatsRows: Vec<PjskProfilePlayStatsRow>,
    pub HasPlayStats: bool,
    pub CharacterRanks: Vec<PjskProfileCharacterRankView>,
    pub HasCharacterRanks: bool,
    pub RadarChart: String,
    pub HasRadarChart: bool,
    pub ChallengeLive: PjskProfileChallengeLiveView,
    pub HasTrainingSection: bool,
    pub FooterExtra: String,
}

// MasterData Structures for honors lookup
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct HonorLevel {
    level: i32,
    description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct HonorEntry {
    id: i32,
    group_id: i32,
    honor_rarity: String,
    name: String,
    levels: Vec<HonorLevel>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct HonorGroup {
    id: i32,
    honor_type: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct BondsHonorLevel {
    level: i32,
    description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct BondsHonor {
    id: i32,
    name: String,
    honor_rarity: String,
    game_character_unit_id1: i32,
    game_character_unit_id2: i32,
    levels: Vec<BondsHonorLevel>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct BondsHonorWord {
    id: i32,
    name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GameCharacterUnit {
    id: i32,
    game_character_id: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GameCharacter {
    id: i32,
    first_name: Option<String>,
    given_name: Option<String>,
}

struct HonorLookup {
    honors: HashMap<i32, HonorEntry>,
    groups: HashMap<i32, HonorGroup>,
    bonds: HashMap<i32, BondsHonor>,
    bond_words: HashMap<i32, BondsHonorWord>,
    character_units: HashMap<i32, GameCharacterUnit>,
    characters: HashMap<i32, GameCharacter>,
}

// Remote profile data structures
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileUser {
    name: Option<String>,
    rank: Option<i32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteUserProfile {
    word: Option<String>,
    twitter_id: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileUserDeck {
    member1: i32,
    member2: i32,
    member3: i32,
    member4: i32,
    member5: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileUserCard {
    card_id: i32,
    default_image: String,
    special_training_status: String,
    level: i32,
    master_rank: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileDifficultyCount {
    music_difficulty_type: String,
    live_clear: i32,
    full_combo: i32,
    all_perfect: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileHonor {
    seq: i32,
    honor_id: i32,
    profile_honor_type: String,
    bonds_honor_word_id: i32,
    honor_level: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileUserCharacter {
    character_id: i32,
    character_rank: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileChallengeSoloStage {
    character_id: i32,
    rank: i32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteProfileChallengeSoloResult {
    character_id: i32,
    high_score: i64,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
struct RemoteProfileResponse {
    user: Option<RemoteProfileUser>,
    user_profile: Option<RemoteUserProfile>,
    user_deck: Option<RemoteProfileUserDeck>,
    #[serde(default)]
    user_cards: Vec<RemoteProfileUserCard>,
    #[serde(default)]
    user_profile_honors: Vec<RemoteProfileHonor>,
    #[serde(default)]
    user_music_difficulty_clear_count: Vec<RemoteProfileDifficultyCount>,
    #[serde(default)]
    user_characters: Vec<RemoteProfileUserCharacter>,
    #[serde(default)]
    user_challenge_live_solo_result: Option<Value>,
    #[serde(default)]
    user_challenge_live_solo_stages: Vec<RemoteProfileChallengeSoloStage>,
    update_time: Option<i64>,
    upload_time: Option<i64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct MasterCard {
    id: i32,
    character_id: i32,
    card_rarity_type: String,
    attribute: String,
    prefix: String,
    assetbundle_name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct CharacterEntryLocal {
    id: i32,
    first_name: Option<String>,
    given_name: Option<String>,
    unit: Option<String>,
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
            let data = match build_profile_page_data(&server, &user_id, prof).await {
                Ok(d) => d,
                Err(e) => {
                    return render_html(
                        "pjsk/profile.html",
                        ProfileResponse {
                            Profile: None,
                            Error: Some(format!("处理 profile 聚合数据失败: {}", e)),
                        },
                    )
                    .into_response();
                }
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

    let resp = crate::pkg::http_client::send(builder)
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

    let resp = crate::pkg::http_client::send(builder)
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

// Master data loader helper
async fn load_honor_lookup(server: &str) -> Result<HonorLookup, String> {
    let honors_raw = read_cached_json(server, "honors.json")?;
    let honors_list: Vec<HonorEntry> = serde_json::from_slice(&honors_raw)
        .map_err(|e| format!("解析 honors.json 失败: {}", e))?;
    let honors: HashMap<i32, HonorEntry> = honors_list.into_iter().map(|h| (h.id, h)).collect();

    let groups_raw = read_cached_json(server, "honorGroups.json")?;
    let groups_list: Vec<HonorGroup> = serde_json::from_slice(&groups_raw)
        .map_err(|e| format!("解析 honorGroups.json 失败: {}", e))?;
    let groups: HashMap<i32, HonorGroup> = groups_list.into_iter().map(|g| (g.id, group_with_compat(g))).collect();

    let bonds_raw = read_cached_json(server, "bondsHonors.json")?;
    let bonds_list: Vec<BondsHonor> = serde_json::from_slice(&bonds_raw)
        .map_err(|e| format!("解析 bondsHonors.json 失败: {}", e))?;
    let bonds: HashMap<i32, BondsHonor> = bonds_list.into_iter().map(|b| (b.id, b)).collect();

    let bond_words = match load_bonds_honor_words_compat(server).await {
        Ok(list) => list.into_iter().map(|w| (w.id, w)).collect(),
        Err(_) => HashMap::new(),
    };

    let units_raw = read_cached_json(server, "gameCharacterUnits.json")?;
    let units_list: Vec<GameCharacterUnit> = serde_json::from_slice(&units_raw)
        .map_err(|e| format!("解析 gameCharacterUnits.json 失败: {}", e))?;
    let character_units: HashMap<i32, GameCharacterUnit> = units_list.into_iter().map(|u| (u.id, u)).collect();

    let chars_raw = read_cached_json(server, "gameCharacters.json")?;
    let chars_list: Vec<GameCharacter> = serde_json::from_slice(&chars_raw)
        .map_err(|e| format!("解析 gameCharacters.json 失败: {}", e))?;
    let characters: HashMap<i32, GameCharacter> = chars_list.into_iter().map(|c| (c.id, c)).collect();

    Ok(HonorLookup {
        honors,
        groups,
        bonds,
        bond_words,
        character_units,
        characters,
    })
}

// Compat mapping for cn arrays vs object array
async fn load_bonds_honor_words_compat(server: &str) -> Result<Vec<BondsHonorWord>, String> {
    let raw = read_cached_json(server, "bondsHonorWords.json")?;
    if let Ok(obj) = serde_json::from_slice::<Vec<BondsHonorWord>>(&raw) {
        return Ok(obj);
    }
    // CN Array list format: [id, seq, bondsGroupId, assetbundleName, name, description]
    if let Ok(arr) = serde_json::from_slice::<Vec<Vec<Value>>>(&raw) {
        let mut words = Vec::new();
        for item in arr {
            if item.len() >= 5 {
                let id = item[0].as_f64().unwrap_or(0.0) as i32;
                let name = item[4].as_str().unwrap_or_default().to_string();
                words.push(BondsHonorWord { id, name });
            }
        }
        return Ok(words);
    }
    Err("无法解析 bondsHonorWords.json".to_string())
}

fn group_with_compat(g: HonorGroup) -> HonorGroup {
    g
}

// Complex aggregate assembler
async fn build_profile_page_data(server: &str, user_id: &str, profile: RemoteProfileResponse) -> Result<PjskProfilePageData, String> {
    let user = profile.user.clone().unwrap_or(RemoteProfileUser { name: None, rank: None });
    let raw_name = user.name.as_deref().unwrap_or("未知玩家");
    let name = if raw_name.trim().is_empty() { "玩家" } else { raw_name };

    let server_name = SERVER_NAMES.get(server).cloned().unwrap_or_else(|| server.to_uppercase());

    let user_profile = profile.user_profile.clone().unwrap_or(RemoteUserProfile { word: None, twitter_id: None });
    let word = match user_profile.word {
        Some(w) => PJSK_PROFILE_WORD_PLACEHOLDER_RE.replace_all(&w, "").trim().to_string(),
        None => String::new(),
    };
    let twitter_id = user_profile.twitter_id.unwrap_or_default().trim().to_string();

    let updated_at = match profile.update_time {
        Some(ts) => format_ts(ts),
        None => match profile.upload_time {
            Some(ts) => format_ts(ts),
            None => String::new(),
        }
    };

    let deck_cards = build_profile_deck(server, &profile.user_deck, &profile.user_cards).await;
    let has_leader_card = !deck_cards.is_empty();
    let leader_card = deck_cards.first().cloned();

    let honors = build_profile_honors(server, &profile.user_profile_honors).await;

    let (difficulty_columns, play_stats_rows) = build_play_stats(profile.user_music_difficulty_clear_count);
    let has_play_stats = !difficulty_columns.is_empty();

    let character_ranks = build_character_ranks(server, &profile.user_characters).await;

    let radar_chart = build_radar_chart(server, &profile.user_characters).await;
    let has_radar_chart = !radar_chart.is_empty();

    let challenge_live = build_challenge_live(server, &profile.user_challenge_live_solo_result, &profile.user_challenge_live_solo_stages).await;
    let has_training_section = has_radar_chart || challenge_live.Available;

    Ok(PjskProfilePageData {
        ServerName: server_name.clone(),
        ServerKey: server.to_string(),
        UserID: user_id.to_string(),
        Name: name.to_string(),
        Rank: user.rank.unwrap_or(1),
        Server: server_name,
        Word: word,
        TwitterID: twitter_id,
        UpdatedAt: updated_at,
        DeckCards: deck_cards,
        HasLeaderCard: has_leader_card,
        LeaderCard: leader_card,
        Honors: honors,
        HasHonors: !profile.user_profile_honors.is_empty(),
        DifficultyColumns: difficulty_columns,
        PlayStatsRows: play_stats_rows,
        HasPlayStats: has_play_stats,
        CharacterRanks: character_ranks,
        HasCharacterRanks: !profile.user_characters.is_empty(),
        RadarChart: radar_chart,
        HasRadarChart: has_radar_chart,
        ChallengeLive: challenge_live,
        HasTrainingSection: has_training_section,
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
    })
}

fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return String::new();
    }
    if ts < 100000000000 {
        crate::pkg::timefmt::format_unix_secs(ts)
    } else {
        crate::pkg::timefmt::format_unix_millis(ts)
    }
}

async fn build_profile_deck(server: &str, deck_opt: &Option<RemoteProfileUserDeck>, cards_list: &[RemoteProfileUserCard]) -> Vec<PjskProfileCardView> {
    let deck = match deck_opt {
        Some(d) => d,
        None => return Vec::new(),
    };
    let card_states: HashMap<i32, &RemoteProfileUserCard> = cards_list.iter().map(|c| (c.card_id, c)).collect();
    let members = [deck.member1, deck.member2, deck.member3, deck.member4, deck.member5];

    let cards_raw = match read_cached_json(server, "cards.json") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let master_cards: Vec<MasterCard> = match serde_json::from_slice(&cards_raw) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let chars_raw = match read_cached_json(server, "gameCharacters.json") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let master_chars: Vec<CharacterEntryLocal> = match serde_json::from_slice(&chars_raw) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut views = Vec::new();
    for mid in members {
        if mid <= 0 {
            continue;
        }
        let card = match master_cards.iter().find(|c| c.id == mid) {
            Some(c) => c,
            None => continue,
        };
        let (char_name, char_unit) = match master_chars.iter().find(|ch| ch.id == card.character_id) {
            Some(ch) => {
                let first = ch.first_name.as_deref().unwrap_or("");
                let given = ch.given_name.as_deref().unwrap_or("");
                let name = if first.is_empty() { given.to_string() } else if given.is_empty() { first.to_string() } else { format!("{} {}", first, given) };
                let unit = ch.unit.as_deref().unwrap_or("");
                (name, unit_name(unit).to_string())
            }
            None => ("未知".to_string(), "".to_string()),
        };

        let (level, master_rank, default_image, special_status) = match card_states.get(&mid) {
            Some(state) => (state.level, state.master_rank, state.default_image.as_str(), state.special_training_status.as_str()),
            None => (1, 0, "normal", "none"),
        };

        let (asset_status, mode_label) = if has_special_training(&card.card_rarity_type) {
            if default_image == "special_training" && special_status.to_lowercase() == "done" {
                ("after_training", "特训后")
            } else {
                ("normal", "特训前")
            }
        } else {
            ("normal", "通常")
        };

        let thumb_label = format!("card:thumbnail:{}:{}", card.assetbundle_name, asset_status);
        let thumbnail = download_asset_by_label(server, &thumb_label).await;

        let star_icon = if card.card_rarity_type == "rarity_birthday" {
            "/static/pjsk/card/rarity_birthday.png".to_string()
        } else {
            "/static/pjsk/card/rarity_star_normal.png".to_string()
        };

        views.push(PjskProfileCardView {
            ID: card.id,
            Prefix: card.prefix.clone(),
            CharacterName: char_name,
            CharacterUnit: char_unit,
            Rarity: rarity_name(&card.card_rarity_type).to_string(),
            Attr: attr_name(&card.attribute).to_string(),
            Level: level,
            MasterRank: master_rank,
            ImageMode: mode_label.to_string(),
            Thumbnail: thumbnail,
            Frame: format!("/static/pjsk/card/cardFrame_S_{}.png", match card.card_rarity_type.as_str() {
                "rarity_1" => "1",
                "rarity_2" => "2",
                "rarity_3" => "3",
                "rarity_4" => "4",
                "rarity_birthday" => "bd",
                _ => "1",
            }),
            AttrIcon: format!("/static/pjsk/card/icon_attribute_{}.png", card.attribute),
            Stars: star_positions(&card.card_rarity_type),
            StarIcon: star_icon,
        });
    }
    views
}

async fn build_profile_honors(server: &str, user_honors: &[RemoteProfileHonor]) -> Vec<PjskProfileHonorView> {
    if user_honors.is_empty() {
        return Vec::new();
    }
    let lookup = match load_honor_lookup(server).await {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };

    let mut views = Vec::new();
    for h in user_honors {
        let seq = if h.seq <= 0 { 1 } else { h.seq };
        let slot = match seq {
            1 => "主头衔",
            2 => "副头衔 1",
            3 => "副头衔 2",
            _ => "头衔",
        };

        let mut view = PjskProfileHonorView {
            Slot: slot.to_string(),
            Title: String::new(),
            Subtitle: String::new(),
            Level: h.honor_level,
            Kind: "普通头衔".to_string(),
            Rarity: "低".to_string(),
            Description: String::new(),
            IsMain: seq == 1,
            HasArtwork: false,
            Artwork: String::new(),
            Width: 0,
            Height: 0,
        };

        if h.profile_honor_type == "bonds" {
            view.Kind = "羁绊头衔".to_string();
            let bonds_honor = match lookup.bonds.get(&h.honor_id) {
                Some(b) => b,
                None => continue,
            };
            view.Title = bonds_honor.name.clone();
            view.Rarity = translate_rarity(&bonds_honor.honor_rarity);

            if h.bonds_honor_word_id > 0 {
                if let Some(word) = lookup.bond_words.get(&h.bonds_honor_word_id) {
                    view.Subtitle = word.name.clone();
                }
            }

            for lvl in &bonds_honor.levels {
                if lvl.level == h.honor_level {
                    view.Description = lvl.description.clone().unwrap_or_default().trim().to_string();
                }
            }

            if view.Subtitle.is_empty() {
                let mut names = Vec::new();
                if let Some(unit1) = lookup.character_units.get(&bonds_honor.game_character_unit_id1) {
                    if let Some(ch) = lookup.characters.get(&unit1.game_character_id) {
                        names.push(short_name(ch));
                    }
                }
                if let Some(unit2) = lookup.character_units.get(&bonds_honor.game_character_unit_id2) {
                    if let Some(ch) = lookup.characters.get(&unit2.game_character_id) {
                        names.push(short_name(ch));
                    }
                }
                view.Subtitle = names.join(" & ");
            }
        } else {
            let normal_honor = match lookup.honors.get(&h.honor_id) {
                Some(nh) => nh,
                None => continue,
            };
            view.Title = normal_honor.name.clone();
            view.Rarity = translate_rarity(&normal_honor.honor_rarity);

            if let Some(group) = lookup.groups.get(&normal_honor.group_id) {
                view.Kind = translate_group_type(&group.honor_type);
            }

            for lvl in &normal_honor.levels {
                if lvl.level == h.honor_level {
                    view.Description = lvl.description.clone().unwrap_or_default().trim().to_string();
                }
            }
        }

        views.push(view);
    }
    views
}

fn translate_rarity(r: &str) -> String {
    match r {
        "low" => "低".to_string(),
        "middle" => "中".to_string(),
        "high" => "高".to_string(),
        "highest" => "最高".to_string(),
        _ => r.to_string(),
    }
}

fn translate_group_type(t: &str) -> String {
    match t {
        "character" => "角色".to_string(),
        "achievement" => "成就".to_string(),
        "event" => "活动".to_string(),
        "limitevent" => "活动应援".to_string(),
        "rank_match" => "排位".to_string(),
        "birthday" => "生日".to_string(),
        "license" => "许可".to_string(),
        "unit_rank" => "团体等级".to_string(),
        "world_bloom" => "世界开花".to_string(),
        "main_story" => "主线剧情".to_string(),
        "challenge_live" => "Challenge Live".to_string(),
        "virtual_live" => "虚拟 Live".to_string(),
        "event_point" => "活动点数".to_string(),
        _ => t.to_string(),
    }
}

fn short_name(ch: &GameCharacter) -> String {
    if let Some(given) = &ch.given_name {
        if !given.trim().is_empty() {
            return given.clone();
        }
    }
    if let Some(first) = &ch.first_name {
        if !first.trim().is_empty() {
            return first.clone();
        }
    }
    format!("角色 #{}", ch.id)
}

fn build_play_stats(counts: Vec<RemoteProfileDifficultyCount>) -> (Vec<PjskProfileDifficultyColumn>, Vec<PjskProfilePlayStatsRow>) {
    let easy = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "easy");
    let normal = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "normal");
    let hard = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "hard");
    let expert = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "expert");
    let master = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "master");
    let append = counts.iter().find(|c| c.music_difficulty_type.to_lowercase() == "append");

    let cols = vec![
        PjskProfileDifficultyColumn { Label: "EZ".to_string(), BackgroundStyle: "background:#5AC06E; color:#ffffff;".to_string(), CellStyle: "background:rgba(90,192,110,0.16); color:#21400d; border-color:rgba(90,192,110,0.35);".to_string() },
        PjskProfileDifficultyColumn { Label: "NM".to_string(), BackgroundStyle: "background:#56A4D4; color:#ffffff;".to_string(), CellStyle: "background:rgba(86,164,212,0.16); color:#12323d; border-color:rgba(86,164,212,0.35);".to_string() },
        PjskProfileDifficultyColumn { Label: "HD".to_string(), BackgroundStyle: "background:#EFAF28; color:#ffffff;".to_string(), CellStyle: "background:rgba(239,175,40,0.16); color:#4b3200; border-color:rgba(239,175,40,0.35);".to_string() },
        PjskProfileDifficultyColumn { Label: "EX".to_string(), BackgroundStyle: "background:#E84D53; color:#ffffff;".to_string(), CellStyle: "background:rgba(232,77,83,0.16); color:#6a1730; border-color:rgba(232,77,83,0.35);".to_string() },
        PjskProfileDifficultyColumn { Label: "MA".to_string(), BackgroundStyle: "background:#BB58B8; color:#ffffff;".to_string(), CellStyle: "background:rgba(187,88,184,0.16); color:#5a1570; border-color:rgba(187,88,184,0.35);".to_string() },
        PjskProfileDifficultyColumn { Label: "APD".to_string(), BackgroundStyle: "background:#EE92BC; color:#ffffff;".to_string(), CellStyle: "background:rgba(238,146,188,0.16); color:#7b3357; border-color:rgba(238,146,188,0.35);".to_string() },
    ];

    let clears = vec![
        PjskProfilePlayStatsCell { Value: easy.map(|c| c.live_clear).unwrap_or(0), Style: cols[0].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: normal.map(|c| c.live_clear).unwrap_or(0), Style: cols[1].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: hard.map(|c| c.live_clear).unwrap_or(0), Style: cols[2].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: expert.map(|c| c.live_clear).unwrap_or(0), Style: cols[3].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: master.map(|c| c.live_clear).unwrap_or(0), Style: cols[4].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: append.map(|c| c.live_clear).unwrap_or(0), Style: cols[5].CellStyle.clone() },
    ];

    let fcs = vec![
        PjskProfilePlayStatsCell { Value: easy.map(|c| c.full_combo).unwrap_or(0), Style: cols[0].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: normal.map(|c| c.full_combo).unwrap_or(0), Style: cols[1].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: hard.map(|c| c.full_combo).unwrap_or(0), Style: cols[2].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: expert.map(|c| c.full_combo).unwrap_or(0), Style: cols[3].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: master.map(|c| c.full_combo).unwrap_or(0), Style: cols[4].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: append.map(|c| c.full_combo).unwrap_or(0), Style: cols[5].CellStyle.clone() },
    ];

    let aps = vec![
        PjskProfilePlayStatsCell { Value: easy.map(|c| c.all_perfect).unwrap_or(0), Style: cols[0].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: normal.map(|c| c.all_perfect).unwrap_or(0), Style: cols[1].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: hard.map(|c| c.all_perfect).unwrap_or(0), Style: cols[2].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: expert.map(|c| c.all_perfect).unwrap_or(0), Style: cols[3].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: master.map(|c| c.all_perfect).unwrap_or(0), Style: cols[4].CellStyle.clone() },
        PjskProfilePlayStatsCell { Value: append.map(|c| c.all_perfect).unwrap_or(0), Style: cols[5].CellStyle.clone() },
    ];

    let rows = vec![
        PjskProfilePlayStatsRow { Label: "CLEAR".to_string(), Values: clears },
        PjskProfilePlayStatsRow { Label: "FC".to_string(), Values: fcs },
        PjskProfilePlayStatsRow { Label: "AP".to_string(), Values: aps },
    ];

    (cols, rows)
}

async fn build_character_ranks(server: &str, user_chars: &[RemoteProfileUserCharacter]) -> Vec<PjskProfileCharacterRankView> {
    let raw = match read_cached_json(server, "gameCharacters.json") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let master_chars: Vec<CharacterEntryLocal> = match serde_json::from_slice(&raw) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let ranks: HashMap<i32, i32> = user_chars.iter().map(|c| (c.character_id, c.character_rank)).collect();

    let mut list = Vec::new();
    for &cid in PJSK_PROFILE_CHARACTER_GRID_ORDER {
        if cid == 0 {
            list.push(PjskProfileCharacterRankView { Name: String::new(), Rank: 0, Empty: true });
            continue;
        }
        let name = match master_chars.iter().find(|ch| ch.id == cid) {
            Some(ch) => {
                let first = ch.first_name.as_deref().unwrap_or("");
                let given = ch.given_name.as_deref().unwrap_or("");
                if given.trim().is_empty() { first.to_string() } else { given.to_string() }
            }
            None => format!("#{}", cid),
        };
        let rank = ranks.get(&cid).cloned().unwrap_or(0);
        list.push(PjskProfileCharacterRankView { Name: name, Rank: rank, Empty: false });
    }
    list
}

async fn build_radar_chart(server: &str, user_chars: &[RemoteProfileUserCharacter]) -> String {
    if user_chars.is_empty() {
        return String::new();
    }
    let raw = match read_cached_json(server, "gameCharacters.json") {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let master_chars: Vec<CharacterEntryLocal> = match serde_json::from_slice(&raw) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let rank_map: HashMap<i32, i32> = user_chars.iter().map(|c| (c.character_id, c.character_rank)).collect();
    let mut max_val = 0;
    for &cid in PJSK_PROFILE_RADAR_ORDER {
        let r = rank_map.get(&cid).cloned().unwrap_or(0);
        if r > max_val {
            max_val = r;
        }
    }
    if max_val <= 0 {
        max_val = 10;
    }
    let max_rank = ((max_val as f64 / 10.0).ceil() * 10.0) as i32;

    let width = 860.0;
    let height = 760.0;
    let center_x = 430.0;
    let center_y = 360.0;
    let outer_radius = 250.0;
    let label_radius = 316.0;
    let val_gap = 18.0;
    let rings = 5;

    let ordered_ids = PJSK_PROFILE_RADAR_ORDER;
    let total_nodes = ordered_ids.len();

    let mut coords = Vec::new();
    let mut value_coords = Vec::new();
    let mut label_coords = Vec::new();

    for (i, &cid) in ordered_ids.iter().enumerate() {
        let angle = -std::f64::consts::FRAC_PI_2 + (2.0 * std::f64::consts::PI * i as f64) / total_nodes as f64;
        let cos_v = angle.cos();
        let sin_v = angle.sin();

        label_coords.push((center_x + cos_v * label_radius, center_y + sin_v * label_radius));

        let r = rank_map.get(&cid).cloned().unwrap_or(0);
        let ratio = (r as f64 / max_rank as f64).clamp(0.0, 1.0);
        let pt_radius = outer_radius * ratio;

        coords.push((center_x + cos_v * pt_radius, center_y + sin_v * pt_radius));
        value_coords.push((center_x + cos_v * (pt_radius + val_gap), center_y + sin_v * (pt_radius + val_gap)));
    }

    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(&mut s, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {:.0} {:.0}" style="display:block;width:100%;height:auto;">"##, width, height);
    s.push_str(r##"<rect x="0" y="0" width="100%" height="100%" rx="24" fill="#ffffff" opacity="0.55"></rect>"##);

    // Draw grid polygons
    for r in (1..=rings).rev() {
        let ratio = r as f64 / rings as f64;
        let fill = if r % 2 == 0 { "rgba(200,224,227,0.20)" } else { "rgba(200,224,227,0.10)" };
        let mut pts = Vec::new();
        for i in 0..total_nodes {
            let angle = -std::f64::consts::FRAC_PI_2 + (2.0 * std::f64::consts::PI * i as f64) / total_nodes as f64;
            let cx = center_x + angle.cos() * (outer_radius * ratio);
            let cy = center_y + angle.sin() * (outer_radius * ratio);
            pts.push(format!("{:.2},{:.2}", cx, cy));
        }
        let _ = write!(&mut s, r##"<polygon points="{}" fill="{}" stroke="rgba(110,110,110,0.16)" stroke-width="1"/>"##, pts.join(" "), fill);
    }

    // Draw straight spokes
    for i in 0..total_nodes {
        let angle = -std::f64::consts::FRAC_PI_2 + (2.0 * std::f64::consts::PI * i as f64) / total_nodes as f64;
        let ax = center_x + angle.cos() * outer_radius;
        let ay = center_y + angle.sin() * outer_radius;
        let _ = write!(&mut s, r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="rgba(110,110,110,0.22)" stroke-width="1"/>"##, center_x, center_y, ax, ay);
    }

    // Label ring values
    for r in 1..=rings {
        let ratio = r as f64 / rings as f64;
        let value = (max_rank as f64 * ratio).round() as i32;
        let y = center_y - outer_radius * ratio;
        let _ = write!(&mut s, r##"<text x="{:.2}" y="{:.2}" fill="rgba(85,85,85,0.68)" font-size="12" text-anchor="middle" dominant-baseline="middle">{}</text>"##, center_x, y - 10.0, value);
    }

    // Draw filled rank area
    let mut fill_pts = Vec::new();
    for (x, y) in &coords {
        fill_pts.push(format!("{:.2},{:.2}", x, y));
    }
    let _ = write!(&mut s, r##"<polygon points="{}" fill="rgba(131,76,117,0.14)" stroke="#834c75" stroke-width="3"/>"##, fill_pts.join(" "));

    // Node dots & rank values & labels
    for (i, &cid) in ordered_ids.iter().enumerate() {
        let name = match master_chars.iter().find(|ch| ch.id == cid) {
            Some(ch) => {
                let first = ch.first_name.as_deref().unwrap_or("");
                let given = ch.given_name.as_deref().unwrap_or("");
                if given.trim().is_empty() { first.to_string() } else { given.to_string() }
            }
            None => format!("#{}", cid),
        };

        let color = resolve_character_color(cid);
        let (lx, ly) = label_coords[i];
        let (px, py) = coords[i];
        let (vx, vy) = value_coords[i];

        let anchor = if lx < center_x - 18.0 { "end" } else if lx > center_x + 18.0 { "start" } else { "middle" };
        let _ = write!(&mut s, r##"<text x="{:.2}" y="{:.2}" fill="{}" font-size="14" font-weight="700" text-anchor="{}" dominant-baseline="middle">{}</text>"##, lx, ly, color, anchor, name);
        let _ = write!(&mut s, r##"<circle cx="{:.2}" cy="{:.2}" r="5.5" fill="{}" stroke="#ffffff" stroke-width="2"/>"##, px, py, color);

        let rank = rank_map.get(&cid).cloned().unwrap_or(0);
        if rank > 0 {
            let _ = write!(&mut s, r##"<text x="{:.2}" y="{:.2}" fill="{}" font-size="12" font-weight="700" text-anchor="middle" dominant-baseline="middle">{}</text>"##, vx, vy, color, rank);
        }
    }

    s.push_str("</svg>");
    s
}

fn resolve_character_color(cid: i32) -> &'static str {
    if let Some(col) = PJSK_PROFILE_CHARACTER_COLORS.get(&cid) {
        return col;
    }
    if cid >= 21 {
        PJSK_PROFILE_UNIT_COLORS.get("piapro").copied().unwrap_or("#33CCBB")
    } else if cid >= 17 {
        PJSK_PROFILE_UNIT_COLORS.get("school_refusal").copied().unwrap_or("#884499")
    } else if cid >= 13 {
        PJSK_PROFILE_UNIT_COLORS.get("theme_park").copied().unwrap_or("#FF9900")
    } else if cid >= 9 {
        PJSK_PROFILE_UNIT_COLORS.get("street").copied().unwrap_or("#EE1166")
    } else if cid >= 5 {
        PJSK_PROFILE_UNIT_COLORS.get("idol").copied().unwrap_or("#88DD44")
    } else {
        PJSK_PROFILE_UNIT_COLORS.get("light_sound").copied().unwrap_or("#4455DD")
    }
}

async fn build_challenge_live(server: &str, raw_opt: &Option<Value>, stages: &[RemoteProfileChallengeSoloStage]) -> PjskProfileChallengeLiveView {
    let result = match raw_opt {
        Some(Value::Array(arr)) if !arr.is_empty() => {
            let mut best: Option<RemoteProfileChallengeSoloResult> = None;
            for val in arr {
                if let Ok(item) = serde_json::from_value::<RemoteProfileChallengeSoloResult>(val.clone()) {
                    if item.character_id > 0 {
                        match &best {
                            Some(b) if item.high_score > b.high_score => best = Some(item),
                            None => best = Some(item),
                            _ => {}
                        }
                    }
                }
            }
            best
        }
        Some(Value::Object(obj)) => {
            serde_json::from_value::<RemoteProfileChallengeSoloResult>(Value::Object(obj.clone())).ok()
        }
        _ => None,
    };

    let best_result = match result {
        Some(r) if r.character_id > 0 => r,
        _ => return PjskProfileChallengeLiveView { Available: false, CharacterName: String::new(), StageRank: 0, HighScore: 0 },
    };

    let mut stage_rank = 0;
    for stage in stages {
        if stage.character_id == best_result.character_id && stage.rank > stage_rank {
            stage_rank = stage.rank;
        }
    }

    let raw = match read_cached_json(server, "gameCharacters.json") {
        Ok(d) => d,
        Err(_) => return PjskProfileChallengeLiveView { Available: true, CharacterName: format!("#{}", best_result.character_id), StageRank: stage_rank, HighScore: best_result.high_score },
    };
    let master_chars: Vec<CharacterEntryLocal> = match serde_json::from_slice(&raw) {
        Ok(c) => c,
        Err(_) => return PjskProfileChallengeLiveView { Available: true, CharacterName: format!("#{}", best_result.character_id), StageRank: stage_rank, HighScore: best_result.high_score },
    };

    let char_name = match master_chars.iter().find(|ch| ch.id == best_result.character_id) {
        Some(ch) => {
            let first = ch.first_name.as_deref().unwrap_or("");
            let given = ch.given_name.as_deref().unwrap_or("");
            if first.is_empty() { given.to_string() } else if given.is_empty() { first.to_string() } else { format!("{} {}", first, given) }
        }
        None => format!("#{}", best_result.character_id),
    };

    PjskProfileChallengeLiveView {
        Available: true,
        CharacterName: char_name,
        StageRank: stage_rank,
        HighScore: best_result.high_score,
    }
}
