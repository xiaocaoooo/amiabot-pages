use axum::{
    extract::Query,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
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
#[allow(non_snake_case)]
pub struct CardDetail {
    pub ID: i32,
    pub Prefix: String, // 对应 Go 兼容：Go 模板使用了 c.Prefix
    pub Title: String,  // Title 用于备用
    pub CharacterName: String,
    pub CharacterUnit: String,
    pub Rarity: String,
    pub Attr: String,
    pub SkillName: String,
    pub FlavorText: String,
    pub ReleaseAt: String,
    pub Server: String,
    pub ServerKey: String,
    pub FooterExtra: String,
    pub Thumbnail: String,
    pub CardImage: String,
    pub Frame: String,
    pub AttrIcon: String,
    pub Stars: Vec<i32>,
    pub StarIcon: String,

    pub CardImageAfter: String,
    pub HasAfter: bool,
    pub ThumbnailAfter: String,
    pub FrameAfter: String,
    pub StarsAfter: Vec<i32>,
    pub StarIconAfter: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct CardResponse {
    pub Card: Option<CardDetail>,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CardEntry {
    id: i32,
    character_id: i32,
    card_rarity_type: String,
    attribute: String,
    prefix: String,
    assetbundle_name: String,
    card_skill_name: Option<String>,
    flavor_text: Option<String>,
    release_at: i64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CharacterEntry {
    id: i32,
    first_name: Option<String>,
    given_name: Option<String>,
    unit: Option<String>,
}

fn format_millis_time(ms: i64) -> String {
    crate::pkg::timefmt::format_unix_millis(ms)
}

pub fn star_positions(rarity: &str) -> Vec<i32> {
    match rarity {
        "rarity_1" => vec![10],
        "rarity_2" => vec![10, 36],
        "rarity_3" => vec![10, 36, 62],
        "rarity_4" => vec![10, 36, 62, 88],
        "rarity_birthday" => vec![10],
        _ => vec![],
    }
}

pub fn has_special_training(rarity: &str) -> bool {
    rarity == "rarity_3" || rarity == "rarity_4" || rarity == "rarity_birthday"
}

pub fn rarity_name(rarity: &str) -> &str {
    match rarity {
        "rarity_1" => "★",
        "rarity_2" => "★★",
        "rarity_3" => "★★★",
        "rarity_4" => "★★★★",
        "rarity_birthday" => "Birthday",
        _ => rarity,
    }
}

pub fn attr_name(attr: &str) -> &str {
    match attr {
        "cool" => "Cool",
        "cute" => "Cute",
        "happy" => "Happy",
        "mysterious" => "Mysterious",
        "pure" => "Pure",
        _ => attr,
    }
}

pub fn unit_name(u: &str) -> &str {
    match u {
        "idol" => "MORE MORE JUMP!",
        "light_sound" => "Leo/need",
        "school_refusal" => "25时、ナイトコード对。",
        "street" => "Vivid BAD SQUAD",
        "theme_park" => "ワンダーランズ×ショウタイム",
        "none" => "混合",
        _ => u,
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

    let (char_name, char_unit) = match characters.iter().find(|ch| ch.id == target_card.character_id) {
        Some(ch) => {
            let first = ch.first_name.as_deref().unwrap_or("");
            let given = ch.given_name.as_deref().unwrap_or("");
            let name = if first.is_empty() {
                given.to_string()
            } else if given.is_empty() {
                first.to_string()
            } else {
                format!("{} {}", first, given)
            };
            let unit = ch.unit.as_deref().unwrap_or("");
            (name, unit_name(unit).to_string())
        }
        None => ("未知".to_string(), "".to_string()),
    };

    let rarity = &target_card.card_rarity_type;
    let attribute = &target_card.attribute;

    // Build assets asynchronously
    let thumb_label = format!("card:thumbnail:{}:normal", target_card.assetbundle_name);
    let thumb = download_asset_by_label(&server, &thumb_label).await;

    let card_image_label = format!("card:image:{}:normal", target_card.assetbundle_name);
    let card_image = download_asset_by_label(&server, &card_image_label).await;

    let star_icon = if rarity == "rarity_birthday" {
        "/static/pjsk/card/rarity_birthday.png".to_string()
    } else {
        "/static/pjsk/card/rarity_star_normal.png".to_string()
    };

    let mut detail = CardDetail {
        ID: target_card.id,
        Prefix: target_card.prefix.clone(),
        Title: target_card.prefix.clone(),
        CharacterName: char_name,
        CharacterUnit: char_unit,
        Rarity: rarity_name(rarity).to_string(),
        Attr: attr_name(attribute).to_string(),
        SkillName: target_card.card_skill_name.clone().unwrap_or_default(),
        FlavorText: target_card.flavor_text.clone().unwrap_or_default(),
        ReleaseAt: format_millis_time(target_card.release_at),
        Server: SERVER_NAMES.get(&server).cloned().unwrap_or_else(|| server.to_uppercase()),
        ServerKey: server.clone(),
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
        Thumbnail: thumb,
        CardImage: card_image,
        Frame: format!("/static/pjsk/card/cardFrame_S_{}.png", match rarity.as_str() {
            "rarity_1" => "1",
            "rarity_2" => "2",
            "rarity_3" => "3",
            "rarity_4" => "4",
            "rarity_birthday" => "bd",
            _ => "1",
        }),
        AttrIcon: format!("/static/pjsk/card/icon_attribute_{}.png", attribute),
        Stars: star_positions(rarity),
        StarIcon: star_icon.clone(),

        CardImageAfter: String::new(),
        HasAfter: false,
        ThumbnailAfter: String::new(),
        FrameAfter: String::new(),
        StarsAfter: vec![],
        StarIconAfter: String::new(),
    };

    if has_special_training(rarity) {
        detail.HasAfter = true;
        let thumb_after_label = format!("card:thumbnail:{}:after_training", target_card.assetbundle_name);
        detail.ThumbnailAfter = download_asset_by_label(&server, &thumb_after_label).await;

        let card_image_after_label = format!("card:image:{}:after_training", target_card.assetbundle_name);
        detail.CardImageAfter = download_asset_by_label(&server, &card_image_after_label).await;

        detail.FrameAfter = detail.Frame.clone();
        detail.StarsAfter = detail.Stars.clone();
        detail.StarIconAfter = star_icon;
    }

    render_html("pjsk/card.html", CardResponse {
        Card: Some(detail),
        Error: None,
    }).into_response()
}
