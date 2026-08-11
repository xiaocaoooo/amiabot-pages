use crate::handlers::pjsk::asset_source::download_asset_by_label;
use crate::handlers::pjsk::assets::read_cached_json;
use crate::handlers::pjsk::card::star_positions;
use crate::handlers::pjsk::{SERVER_NAMES, VALID_SERVERS};
use crate::handlers::render_html;
use axum::{extract::Query, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct EventQuery {
    pub id: Option<String>,
    pub server: Option<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct CardView {
    pub ID: i32,
    pub Prefix: String,
    pub Thumbnail: String,
    pub Frame: String,
    pub AttrIcon: String,
    pub Stars: Vec<i32>,
    pub StarIcon: String,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct EventPageData {
    pub ID: i32,
    pub Name: String,
    pub EventType: String,
    pub StartAt: String,
    pub AggregateAt: String,
    pub ClosedAt: String,
    pub Server: String,
    pub ServerKey: String,
    pub Banner: String,
    pub Logo: String,
    pub Status: &'static str,
    pub Progress: String,
    pub Cards: Vec<CardView>,
    pub FooterExtra: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct EventResponse {
    pub Event: Option<EventPageData>,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct EventEntry {
    id: i32,
    name: String,
    eventType: String,
    assetbundleName: String,
    startAt: i64,
    aggregateAt: i64,
    closedAt: i64,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct CardEntry {
    id: i32,
    prefix: String,
    assetbundleName: String,
    cardRarityType: String,
    attribute: String,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct EventCardsEntry {
    eventId: i32,
    cardId: i32,
}

fn format_millis_time(ms: i64) -> String {
    crate::pkg::timefmt::format_unix_millis(ms)
}

fn event_status(start: i64, close: i64) -> &'static str {
    let now = chrono::Local::now().timestamp_millis();
    if now < start {
        "未开始"
    } else if now > close {
        "已结束"
    } else {
        "进行中"
    }
}

fn event_progress(start: i64, close: i64) -> f64 {
    let now = chrono::Local::now().timestamp_millis();
    if now < start {
        return 0.0;
    }
    if now > close {
        return 100.0;
    }
    let total = (close - start) as f64;
    if total <= 0.0 {
        return 100.0;
    }
    ((now - start) as f64 / total * 100.0).min(100.0).max(0.0)
}

pub async fn event_handler(Query(q): Query<EventQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return render_html(
            "pjsk/event.html",
            EventResponse {
                Event: None,
                Error: Some("无效的服务器参数，支持: jp, cn, en, tw, kr".to_string()),
            },
        )
        .into_response();
    }

    let event_id_str = q.id.unwrap_or_default().trim().to_string();
    if event_id_str.is_empty() {
        return render_html(
            "pjsk/event.html",
            EventResponse {
                Event: None,
                Error: Some("缺少活动 ID 参数".to_string()),
            },
        )
        .into_response();
    }

    let event_id: i32 = match event_id_str.parse() {
        Ok(id) if id > 0 => id,
        _ => {
            return render_html(
                "pjsk/event.html",
                EventResponse {
                    Event: None,
                    Error: Some("无效的活动 ID".to_string()),
                },
            )
            .into_response();
        }
    };

    // Load event masterdata
    let events_data = match read_cached_json(&server, "events.json") {
        Ok(d) => d,
        Err(e) => {
            return render_html(
                "pjsk/event.html",
                EventResponse {
                    Event: None,
                    Error: Some(e),
                },
            )
            .into_response()
        }
    };
    let events: Vec<EventEntry> = match serde_json::from_slice(&events_data) {
        Ok(evs) => evs,
        Err(e) => {
            return render_html(
                "pjsk/event.html",
                EventResponse {
                    Event: None,
                    Error: Some(format!("解析 events.json 失败: {}", e)),
                },
            )
            .into_response()
        }
    };

    let target = match events.iter().find(|e| e.id == event_id) {
        Some(e) => e,
        None => {
            return render_html(
                "pjsk/event.html",
                EventResponse {
                    Event: None,
                    Error: Some(format!("未找到活动 #{}", event_id)),
                },
            )
            .into_response()
        }
    };

    // Parse event type (marathon, cheerful_carnival etc.)
    let ev_type = match target.eventType.as_str() {
        "marathon" => "马拉松活动",
        "cheerful_carnival" => "欢乐狂欢节活动",
        other => other,
    };

    // Retrieve related cards from eventDeckCards.json & cards.json
    let mut cards = Vec::new();
    if let Ok(deck_data) = read_cached_json(&server, "eventDeckCards.json") {
        if let Ok(deck_cards) = serde_json::from_slice::<Vec<EventCardsEntry>>(&deck_data) {
            let card_ids: Vec<i32> = deck_cards
                .iter()
                .filter(|dc| dc.eventId == event_id)
                .map(|dc| dc.cardId)
                .collect();

            if let Ok(cards_data) = read_cached_json(&server, "cards.json") {
                if let Ok(all_cards) = serde_json::from_slice::<Vec<CardEntry>>(&cards_data) {
                    for cid in card_ids {
                        if let Some(c) = all_cards.iter().find(|c| c.id == cid) {
                            let label = format!("card:thumbnail:{}:normal", c.assetbundleName);
                            let thumb = download_asset_by_label(&server, &label).await;

                            let rarity = &c.cardRarityType;
                            let star_icon = if rarity == "rarity_birthday" {
                                "/static/pjsk/card/rarity_birthday.png".to_string()
                            } else {
                                "/static/pjsk/card/rarity_star_normal.png".to_string()
                            };

                            let frame = format!(
                                "/static/pjsk/card/cardFrame_S_{}.png",
                                match rarity.as_str() {
                                    "rarity_1" => "1",
                                    "rarity_2" => "2",
                                    "rarity_3" => "3",
                                    "rarity_4" => "4",
                                    "rarity_birthday" => "bd",
                                    _ => "1",
                                }
                            );

                            cards.push(CardView {
                                ID: c.id,
                                Prefix: c.prefix.clone(),
                                Thumbnail: thumb,
                                Frame: frame,
                                AttrIcon: format!(
                                    "/static/pjsk/card/icon_attribute_{}.png",
                                    c.attribute
                                ),
                                Stars: star_positions(rarity),
                                StarIcon: star_icon,
                            });
                        }
                    }
                }
            }
        }
    }

    let banner_label = format!("event:banner:{}", target.assetbundleName);
    let banner = download_asset_by_label(&server, &banner_label).await;

    let logo_label = format!("event:logo:{}", target.assetbundleName);
    let logo = download_asset_by_label(&server, &logo_label).await;

    let status = event_status(target.startAt, target.closedAt);
    let progress = format!("{:.1}", event_progress(target.startAt, target.closedAt));

    let page = EventPageData {
        ID: target.id,
        Name: target.name.clone(),
        EventType: ev_type.to_string(),
        StartAt: format_millis_time(target.startAt),
        AggregateAt: format_millis_time(target.aggregateAt),
        ClosedAt: format_millis_time(target.closedAt),
        Server: SERVER_NAMES
            .get(&server)
            .cloned()
            .unwrap_or_else(|| server.to_uppercase()),
        ServerKey: server,
        Banner: banner,
        Logo: logo,
        Status: status,
        Progress: progress,
        Cards: cards,
        FooterExtra: "Powered by Moesekai, Haruki, LunaBot, Uni, & Sekai World<br />".to_string(),
    };

    render_html(
        "pjsk/event.html",
        EventResponse {
            Event: Some(page),
            Error: None,
        },
    )
    .into_response()
}

pub async fn current_event_handler(Query(q): Query<EventQuery>) -> impl IntoResponse {
    let server = q.server.unwrap_or_else(|| "jp".to_string());
    if !VALID_SERVERS.contains(&server) {
        return (StatusCode::BAD_REQUEST, "无效服务器").into_response();
    }

    let events_data = match read_cached_json(&server, "events.json") {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let events: Vec<EventEntry> = match serde_json::from_slice(&events_data) {
        Ok(evs) => evs,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("解析 events.json 失败: {}", e),
            )
                .into_response()
        }
    };

    let now = chrono::Local::now().timestamp_millis();
    let current = events
        .iter()
        .find(|e| now >= e.startAt && now <= e.closedAt);

    let redirect_id = match current {
        Some(e) => e.id,
        None => {
            // Find the latest ended event
            events
                .iter()
                .filter(|e| now > e.closedAt)
                .max_by_key(|e| e.closedAt)
                .map(|e| e.id)
                .unwrap_or(1)
        }
    };

    let redirect_url = format!("/pjsk/event?id={}&server={}", redirect_id, server);
    (
        StatusCode::FOUND,
        [(axum::http::header::LOCATION, redirect_url.as_str())],
    )
        .into_response()
}
