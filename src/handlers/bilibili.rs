use axum::{
    extract::Query,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;
use crate::handlers::{render_html, format_upstream_http_error};

#[derive(Deserialize, Debug)]
pub struct BilibiliQuery {
    pub bv: Option<String>,
    pub bvid: Option<String>,
    pub av: Option<String>,
    pub aid: Option<String>,
    pub avid: Option<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct ViewPage {
    pub Index: i32,
    pub Title: String,
    pub DurationStr: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct BilibiliPageData {
    pub Title: String,
    pub BVID: String,
    pub AID: i64,
    pub Cover: String,
    pub UpperName: String,
    pub UpperFace: String,
    pub Play: String,
    pub Like: String,
    pub Coin: String,
    pub Favorite: String,
    pub Danmaku: String,
    pub Reply: String,
    pub Share: String,
    pub Duration: String,
    pub PublishTime: String,
    pub UploadTime: String,
    pub ShowUploadTime: bool,
    pub Intro: String,
    pub Pages: Vec<ViewPage>,
    pub TotalPages: usize,
    pub HasMorePages: bool,
    pub RemainingPageCount: usize,
    pub FooterExtra: String,
    pub Error: Option<String>,
}

#[derive(Deserialize, Debug)]
struct BiliViewResp {
    code: i32,
    message: String,
    data: Option<BiliViewData>,
}

#[derive(Deserialize, Debug)]
struct BiliViewData {
    aid: i64,
    bvid: String,
    title: String,
    pic: String,
    desc: String,
    pubdate: i64,
    ctime: i64,
    duration: i32,
    owner: BiliOwner,
    stat: BiliStat,
    pages: Vec<BiliPage>,
}

#[derive(Deserialize, Debug)]
struct BiliOwner {
    name: String,
    face: String,
}

#[derive(Deserialize, Debug)]
struct BiliStat {
    view: i64,
    danmaku: i64,
    reply: i64,
    favorite: i64,
    coin: i64,
    share: i64,
    like: i64,
}

#[derive(Deserialize, Debug)]
struct BiliPage {
    page: i32,
    part: String,
    duration: i32,
}

fn format_count(v: i64) -> String {
    if v >= 100_000_000 {
        format!("{:.1}亿", v as f64 / 100_000_000.0)
    } else if v >= 10_000 {
        format!("{:.1}万", v as f64 / 10_000.0)
    } else {
        v.to_string()
    }
}

fn format_duration(sec: i32) -> String {
    let sec = sec.max(0);
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    let s = sec % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

fn format_unix_time(ts: i64) -> String {
    crate::pkg::timefmt::format_unix_secs(ts)
}

fn render_error(msg: &str) -> impl IntoResponse {
    render_html("bilibili/video.html", BilibiliPageData {
        Title: String::new(),
        BVID: String::new(),
        AID: 0,
        Cover: String::new(),
        UpperName: String::new(),
        UpperFace: String::new(),
        Play: String::new(),
        Like: String::new(),
        Coin: String::new(),
        Favorite: String::new(),
        Danmaku: String::new(),
        Reply: String::new(),
        Share: String::new(),
        Duration: String::new(),
        PublishTime: String::new(),
        UploadTime: String::new(),
        ShowUploadTime: false,
        Intro: String::new(),
        Pages: Vec::new(),
        TotalPages: 0,
        HasMorePages: false,
        RemainingPageCount: 0,
        FooterExtra: String::new(),
        Error: Some(msg.to_string()),
    })
}

fn render_default_video() -> impl IntoResponse {
    render_html("bilibili/video.html", BilibiliPageData {
        Title: "Bilibili 视频信息示例".to_string(),
        BVID: "BV1XY411A7c2".to_string(),
        AID: 0,
        Cover: String::new(),
        UpperName: "AmiaBot".to_string(),
        UpperFace: String::new(),
        Play: "--".to_string(),
        Like: "--".to_string(),
        Coin: "--".to_string(),
        Favorite: "--".to_string(),
        Danmaku: "--".to_string(),
        Reply: "--".to_string(),
        Share: "--".to_string(),
        Duration: "--:--".to_string(),
        PublishTime: String::new(),
        UploadTime: String::new(),
        ShowUploadTime: false,
        Intro: "未提供 av/bv 参数，当前展示为默认示例数据。可使用 /bilibili/video?bv=BV号 或 /bilibili/video?av=AV号 进行查询。".to_string(),
        Pages: Vec::new(),
        TotalPages: 0,
        HasMorePages: false,
        RemainingPageCount: 0,
        FooterExtra: String::new(),
        Error: None,
    })
}

pub async fn video_handler(Query(q): Query<BilibiliQuery>) -> impl IntoResponse {
    let bv = q.bv.or(q.bvid);
    let av = q.av.or(q.aid).or(q.avid);
    
    if bv.is_none() && av.is_none() {
        return render_default_video().into_response();
    }

    let mut query_params = Vec::new();
    if let Some(ref b) = bv {
        query_params.push(("bvid", b.clone()));
    } else if let Some(ref a) = av {
        query_params.push(("aid", a.clone()));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default();

    let res = crate::pkg::http_client::send(
        client.get("https://api.bilibili.com/x/web-interface/view")
            .query(&query_params)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36")
    ).await;

    let resp = match res {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("请求 Bilibili 接口失败: {}", e);
            tracing::warn!(error = %msg, "bilibili 请求失败");
            return render_error(&msg).into_response();
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = format_upstream_http_error("Bilibili 接口", status, &body);
        tracing::warn!(error = %msg, "bilibili 上游失败");
        return render_error(&msg).into_response();
    }

    let api_resp = match resp.json::<BiliViewResp>().await {
        Ok(p) => p,
        Err(e) => {
        let msg = format!("解析 Bilibili 返回数据失败: {}", e);
        tracing::warn!(error = %msg, "bilibili 解析失败");
        return render_error(&msg).into_response();
    },
    };

    if api_resp.code != 0 {
        {
        let msg = format!("Bilibili 接口错误: {} (code={})", api_resp.message, api_resp.code);
        tracing::warn!(error = %msg, "bilibili 业务错误");
        return render_error(&msg).into_response();
    }
    }

    let data = match api_resp.data {
        Some(d) => d,
        None => return render_error("Bilibili 接口未返回数据域").into_response(),
    };

    let mut pages = Vec::new();
    for (i, p) in data.pages.iter().enumerate() {
        if i >= 8 {
            break;
        }
        pages.push(ViewPage {
            Index: p.page,
            Title: p.part.clone(),
            DurationStr: format_duration(p.duration),
        });
    }

    let mut headers = HashMap::new();
    headers.insert("Referer".to_string(), "https://www.bilibili.com/".to_string());
    
    let cover_data_url = DEFAULT_IMG_CACHE.download(&data.pic, None, Some(&headers)).await;
    let upper_face_data_url = DEFAULT_IMG_CACHE.download(&data.owner.face, None, Some(&headers)).await;

    let publish_time = format_unix_time(data.pubdate);
    let upload_time = format_unix_time(data.ctime);
    let mut show_upload_time = false;

    if data.pubdate > 0 && data.ctime > 0 {
        let delta = (data.pubdate - data.ctime).abs();
        show_upload_time = delta >= 30 * 60;
    }

    render_html("bilibili/video.html", BilibiliPageData {
        Title: data.title,
        BVID: data.bvid,
        AID: data.aid,
        Cover: cover_data_url,
        UpperName: data.owner.name,
        UpperFace: upper_face_data_url,
        Play: format_count(data.stat.view),
        Like: format_count(data.stat.like),
        Coin: format_count(data.stat.coin),
        Favorite: format_count(data.stat.favorite),
        Danmaku: format_count(data.stat.danmaku),
        Reply: format_count(data.stat.reply),
        Share: format_count(data.stat.share),
        Duration: format_duration(data.duration),
        PublishTime: publish_time,
        UploadTime: upload_time,
        ShowUploadTime: show_upload_time,
        Intro: data.desc,
        Pages: pages,
        TotalPages: data.pages.len(),
        HasMorePages: data.pages.len() > 8,
        RemainingPageCount: data.pages.len().saturating_sub(8),
        FooterExtra: String::new(),
        Error: None,
    }).into_response()
}
