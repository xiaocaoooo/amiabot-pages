use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::pjsk::assets::read_cached_json;
use crate::handlers::pjsk::{SERVER_NAMES, VALID_SERVERS};
use crate::handlers::render_html;
use axum::{extract::Query, response::IntoResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct MusicQuery {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct VocalInfo {
    pub VocalistType: String,
    pub Name: String,
    pub Characters: Vec<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct DifficultyInfo {
    pub DifficultyType: String,
    pub PlayLevel: i32,
    pub NoteCount: i32,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct EventInfo {
    pub EventID: i32,
    pub EventName: String,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct MusicDetail {
    pub ID: i32,
    pub Title: String,
    pub Pronunciation: String,
    pub Creator: String, // Lyricist/Composer/Arranger
    pub Lyricist: String,
    pub Composer: String,
    pub Arranger: String,
    pub PublishAt: String,
    pub Server: String,
    pub ServerKey: String,
    pub Jacket: String,
    pub Vocalists: Vec<VocalInfo>,
    pub Difficulties: Vec<DifficultyInfo>,
    pub Events: Vec<EventInfo>,
    pub FooterExtra: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct MusicResponse {
    pub Music: Option<MusicDetail>,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct MusicEntry {
    id: i32,
    title: String,
    pronunciation: String,
    lyricist: String,
    composer: String,
    arranger: String,
    assetbundleName: String,
    publishedAt: i64,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct MusicDifficultyEntry {
    musicId: i32,
    musicDifficulty: String,
    playLevel: i32,
    noteCount: i32,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct MusicVocalEntry {
    musicId: i32,
    musicVocalistType: String,
    assetbundleName: String,
}

fn format_millis_time(ms: i64) -> String {
    crate::pkg::timefmt::format_unix_millis(ms)
}

pub async fn music_handler(Query(q): Query<MusicQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_html(
            "pjsk/music.html",
            MusicResponse {
                Music: None,
                Error: Some("无效的服务器参数，支持: jp, cn, en, tw, kr".to_string()),
            },
        )
        .into_response();
    }

    let music_id_str = q.id.unwrap_or_default().trim().to_string();
    if music_id_str.is_empty() {
        return render_html(
            "pjsk/music.html",
            MusicResponse {
                Music: None,
                Error: Some("缺少音乐 ID 参数".to_string()),
            },
        )
        .into_response();
    }

    let music_id: i32 = match music_id_str.parse() {
        Ok(id) if id > 0 => id,
        _ => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some("无效的音乐 ID".to_string()),
                },
            )
            .into_response();
        }
    };

    // Load music masterdata
    let musics_data = match read_cached_json(&server, "musics.json") {
        Ok(d) => d,
        Err(e) => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some(e),
                },
            )
            .into_response()
        }
    };
    let musics: Vec<MusicEntry> = match serde_json::from_slice(&musics_data) {
        Ok(m) => m,
        Err(e) => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some(format!("解析 musics.json 失败: {}", e)),
                },
            )
            .into_response()
        }
    };

    let target = match musics.iter().find(|m| m.id == music_id) {
        Some(m) => m,
        None => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some(format!("未找到音乐 #{}", music_id)),
                },
            )
            .into_response()
        }
    };

    // Load music difficulties
    let diffs_data = match read_cached_json(&server, "musicDifficulties.json") {
        Ok(d) => d,
        Err(e) => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some(e),
                },
            )
            .into_response()
        }
    };
    let all_diffs: Vec<MusicDifficultyEntry> = match serde_json::from_slice(&diffs_data) {
        Ok(d) => d,
        Err(e) => {
            return render_html(
                "pjsk/music.html",
                MusicResponse {
                    Music: None,
                    Error: Some(format!("解析 musicDifficulties.json 失败: {}", e)),
                },
            )
            .into_response()
        }
    };
    let difficulties: Vec<DifficultyInfo> = all_diffs
        .iter()
        .filter(|d| d.musicId == music_id)
        .map(|d| {
            let label = match d.musicDifficulty.as_str() {
                "easy" => "EASY",
                "normal" => "NORMAL",
                "hard" => "HARD",
                "expert" => "EXPERT",
                "master" => "MASTER",
                "append" => "APPEND",
                other => other,
            };
            DifficultyInfo {
                DifficultyType: label.to_string(),
                PlayLevel: d.playLevel,
                NoteCount: d.noteCount,
            }
        })
        .collect();

    // Load vocalists
    let mut vocalists = Vec::new();
    if let Ok(vocals_data) = read_cached_json(&server, "musicVocals.json") {
        if let Ok(vocals) = serde_json::from_slice::<Vec<MusicVocalEntry>>(&vocals_data) {
            for v in vocals.iter().filter(|v| v.musicId == music_id) {
                let name = match v.musicVocalistType.as_str() {
                    "vocaloid" => "虚拟歌手".to_string(),
                    "light_music_club" => "原创歌手".to_string(),
                    _ => "世界联合".to_string(),
                };
                vocalists.push(VocalInfo {
                    VocalistType: v.musicVocalistType.clone(),
                    Name: name,
                    Characters: Vec::new(),
                });
            }
        }
    }

    let jacket_label = format!("music:jacket:{}", target.assetbundleName);
    let jacket = download_asset_by_label(&server, &jacket_label).await;

    let creator = format!(
        "词: {} / 曲: {} / 编: {}",
        target.lyricist, target.composer, target.arranger
    );

    let detail = MusicDetail {
        ID: target.id,
        Title: target.title.clone(),
        Pronunciation: target.pronunciation.clone(),
        Creator: creator,
        Lyricist: target.lyricist.clone(),
        Composer: target.composer.clone(),
        Arranger: target.arranger.clone(),
        PublishAt: format_millis_time(target.publishedAt),
        Server: SERVER_NAMES
            .get(&server)
            .cloned()
            .unwrap_or_else(|| server.to_uppercase()),
        ServerKey: server,
        Jacket: jacket,
        Vocalists: vocalists,
        Difficulties: difficulties,
        Events: Vec::new(), // Populate if needed
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
    };

    render_html(
        "pjsk/music.html",
        MusicResponse {
            Music: Some(detail),
            Error: None,
        },
    )
    .into_response()
}
