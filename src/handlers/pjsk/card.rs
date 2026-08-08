use axum::{
    extract::Query,
    response::IntoResponse,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::handlers::pjsk::{VALID_SERVERS, SERVER_NAMES};
use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::pjsk::assets::read_cached_json;
use crate::handlers::render_html;

#[derive(Deserialize, Debug)]
pub struct CardQuery {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct CardDetail {
    pub ID: i32,
    pub Title: String,
    pub CharacterName: String,
    pub Rarity: String,
    pub Attr: String,
    pub ReleaseAt: String,
    pub Server: String,
    pub ServerKey: String,
    pub FooterExtra: String,
    pub Thumbnail: String,
    pub CardImage: String,
    pub Frame: String,
    pub AttrIcon: String,
}

#[derive(Serialize)]
pub struct CardResponse {
    pub Card: Option<CardDetail>,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CardEntry {
    id: i32,
    characterId: i32,
    cardRarityType: String,
    attribute: String,
    prefix: String,
    assetbundleName: String,
    releaseAt: i64,
}

#[derive(Deserialize, Debug)]
struct CharacterEntry {
    id: i32,
    firstName: Option<String>,
    givenName: Option<String>,
}

fn format_millis_time(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    if let Some(dt) = chrono::NaiveDateTime::from_timestamp_opt(ms / 1000, 0) {
        let local_dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from_naive_utc_and_offset(
            dt,
            *chrono::Local::now().offset()
        );
        local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        String::new()
    }
}

pub async fn card_handler(Query(q): Query<CardQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_html("pjsk/card.html", CardResponse {
            Card: None,
            Error: Some("无效的服务器参数，支持: jp, cn, en, tw, kr".to_string()),
        }).into_response();
    }

    let card_id_str = q.id.unwrap_or_default().trim().to_string();
    if card_id_str.is_empty() {
        return render_html("pjsk/card.html", CardResponse {
            Card: None,
            Error: Some("缺少卡片 ID 参数".to_string()),
        }).into_response();
    }

    let card_id: i32 = match card_id_str.parse() {
        Ok(id) if id > 0 => id,
        _ => {
            return render_html("pjsk/card.html", CardResponse {
                Card: None,
                Error: Some("无效的卡片 ID".to_string()),
            }).into_response();
        }
    };

    // Load card detail from local masterdata
    let cards_data = match read_cached_json(&server, "cards.json") {
        Ok(d) => d,
        Err(e) => return render_html("pjsk/card.html", CardResponse { Card: None, Error: Some(e) }).into_response(),
    };
    let cards: Vec<CardEntry> = match serde_json::from_slice(&cards_data) {
        Ok(c) => c,
        Err(e) => return render_html("pjsk/card.html", CardResponse { Card: None, Error: Some(format!("解析 cards.json 失败: {}", e)) }).into_response(),
    };

    let target_card = match cards.iter().find(|c| c.id == card_id) {
        Some(c) => c,
        None => return render_html("pjsk/card.html", CardResponse { Card: None, Error: Some(format!("未找到卡片 #{}", card_id)) }).into_response(),
    };

    // Fetch character details
    let chars_data = match read_cached_json(&server, "gameCharacters.json") {
        Ok(d) => d,
        Err(e) => return render_html("pjsk/card.html", CardResponse { Card: None, Error: Some(e) }).into_response(),
    };
    let characters: Vec<CharacterEntry> = match serde_json::from_slice(&chars_data) {
        Ok(c) => c,
        Err(e) => return render_html("pjsk/card.html", CardResponse { Card: None, Error: Some(format!("解析 gameCharacters.json 失败: {}", e)) }).into_response(),
    };

    let char_name = match characters.iter().find(|ch| ch.id == target_card.characterId) {
        Some(ch) => {
            let first = ch.firstName.as_deref().unwrap_or("");
            let given = ch.givenName.as_deref().unwrap_or("");
            if first.is_empty() {
                given.to_string()
            } else if given.is_empty() {
                first.to_string()
            } else {
                format!("{} {}", first, given)
            }
        }
        None => "未知".to_string(),
    };

    let rarity = &target_card.cardRarityType;
    let attribute = &target_card.attribute;

    // Build assets asynchronously
    let thumb_label = format!("card:thumbnail:{}:normal", target_card.assetbundleName);
    let thumb = download_asset_by_label(&server, &thumb_label).await;

    let card_image_label = format!("card:image:{}:normal", target_card.assetbundleName);
    let card_image = download_asset_by_label(&server, &card_image_label).await;

    // Use placeholder values for frame or attribute icons (as configured in static files or remote)
    let frame_url = format!("/static/pjsk/card/frame_{}.png", rarity);
    let attr_url = format!("/static/pjsk/card/icon_attr_{}.png", attribute);

    let detail = CardDetail {
        ID: target_card.id,
        Title: target_card.prefix.clone(),
        CharacterName: char_name,
        Rarity: rarity.clone(),
        Attr: attribute.clone(),
        ReleaseAt: format_millis_time(target_card.releaseAt),
        Server: SERVER_NAMES.get(&server).cloned().unwrap_or_else(|| server.to_uppercase()),
        ServerKey: server,
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
        Thumbnail: thumb,
        CardImage: card_image,
        Frame: frame_url,
        AttrIcon: attr_url,
    };

    render_html("pjsk/card.html", CardResponse {
        Card: Some(detail),
        Error: None,
    }).into_response()
}
