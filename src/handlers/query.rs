use crate::handlers::render_html;
use crate::pkg::imgcache::DEFAULT_IMG_CACHE;
use axum::{extract::Query, response::IntoResponse};
use chrono::Datelike;
use serde::{Deserialize, Serialize};

const QUERY_TEXT_LIMIT: usize = 180;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub struct QueryParams {
    pub id: Option<String>,
    pub nickname: Option<String>,
    pub card: Option<String>,
    pub role: Option<String>,
    pub title: Option<String>,
    pub is_vip: Option<String>,
    pub is_years_vip: Option<String>,
    pub vip_level: Option<String>,
    pub online_status: Option<String>,
    pub online_ext_status: Option<String>,
    pub remark: Option<String>,
    pub category_name: Option<String>,
    pub category_id: Option<String>,
    pub qid: Option<String>,
    pub sex: Option<String>,
    pub age: Option<String>,
    pub qage: Option<String>,
    pub reg_year: Option<String>,
    pub qq_level: Option<String>,
    pub birthday: Option<String>,
    pub phone_num: Option<String>,
    pub email: Option<String>,
    pub group_level: Option<String>,
    pub area: Option<String>,
    pub is_robot: Option<String>,
    pub unfriendly: Option<String>,
    pub join_time: Option<String>,
    pub last_sent_time: Option<String>,
    pub mute_until: Option<String>,
    pub title_expire_time: Option<String>,
    pub long_nick: Option<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DisplayItem {
    pub Label: String,
    pub Value: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DisplaySection {
    pub Title: String,
    pub GridTemplate: String,
    pub Items: Vec<DisplayItem>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct TextBlock {
    pub Title: String,
    pub Value: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct UserPageData {
    pub Avatar: String,
    pub DisplayName: String,
    pub ID: String,
    pub Badges: Vec<String>,
    pub Sections: Vec<DisplaySection>,
    pub TextBlocks: Vec<TextBlock>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct UserResponse {
    pub User: Option<UserPageData>,
    pub Error: Option<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct GroupPageData {
    pub Avatar: String,
    pub Name: String,
    pub ID: String,
    pub Badges: Vec<String>,
    pub Sections: Vec<DisplaySection>,
    pub TextBlocks: Vec<TextBlock>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct GroupResponse {
    pub Group: Option<GroupPageData>,
    pub Error: Option<String>,
}

fn is_digits(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_digit())
}

fn clamp_text(raw: &str, limit: usize) -> String {
    let raw = raw.trim();
    if raw.is_empty() || limit == 0 {
        return raw.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= limit {
        return raw.to_string();
    }
    let mut s: String = chars[..limit].iter().collect();
    s.push('…');
    s
}

fn first_non_empty(values: &[&str]) -> String {
    for val in values {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn non_empty_or(val: &str, fallback: &str) -> String {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_positive_int(raw: &str) -> i32 {
    raw.trim().parse::<i32>().unwrap_or(0).max(0)
}

fn parse_optional_bool(raw: &str) -> (bool, bool) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (false, false);
    }
    match raw.parse::<bool>() {
        Ok(b) => (b, true),
        Err(_) => (false, false),
    }
}

fn humanize_sex(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "male" => "男",
        "female" => "女",
        _ => "未知",
    }
}

fn humanize_role(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "owner" => "群主",
        "admin" => "管理员",
        "member" => "成员",
        _ => "未提供",
    }
}

fn humanize_int(value: i32, empty: &str, suffix: &str) -> String {
    if value <= 0 {
        empty.to_string()
    } else {
        format!("{}{}", value, suffix)
    }
}

fn humanize_q_age(reg_year: i32) -> String {
    if reg_year <= 0 {
        return "未知".to_string();
    }
    let now_year = chrono::Local::now().date_naive().year();
    if reg_year > now_year {
        return "未知".to_string();
    }
    let q_age = now_year - reg_year;
    format!("{} 年", q_age)
}

fn humanize_vip(is_vip_raw: &str, is_years_vip_raw: &str, level: i32) -> String {
    let (is_vip, has_vip) = parse_optional_bool(is_vip_raw);
    let (is_years_vip, has_years_vip) = parse_optional_bool(is_years_vip_raw);
    if !has_vip && !has_years_vip && level <= 0 {
        return "未提供".to_string();
    }
    let mut parts = Vec::new();
    if is_vip {
        parts.push("VIP");
    }
    if is_years_vip {
        parts.push("年费");
    }
    if level > 0 {
        parts.push("Lv.");
    }
    if parts.is_empty() {
        return "普通用户".to_string();
    }
    let mut out = parts.join(" · ");
    if level > 0 {
        out = format!("{} {}", out, level);
    }
    out
}

fn humanize_optional_bool(raw: &str) -> &'static str {
    let (val, ok) = parse_optional_bool(raw);
    if !ok {
        "未提供"
    } else if val {
        "是"
    } else {
        "否"
    }
}

fn humanize_online_status(status: i32, ext_status: i32) -> String {
    let status_map: std::collections::HashMap<i32, &str> = [
        (10, "在线"),
        (30, "离开"),
        (40, "隐身"),
        (50, "忙碌"),
        (60, "Q我吧"),
        (70, "请勿打扰"),
    ]
    .into_iter()
    .collect();

    let ext_map: std::collections::HashMap<i32, &str> = [
        (1000, "我的电量"),
        (1011, "信号弱"),
        (1016, "睡觉中"),
        (1018, "学习中"),
        (1021, "追剧中"),
        (1027, "timi中"),
        (1028, "听歌中"),
        (1030, "今日天气"),
        (1032, "熬夜中"),
        (1051, "恋爱中"),
        (1052, "我没事"),
        (1056, "嗨到飞起"),
        (1058, "元气满满"),
        (1059, "悠哉哉"),
        (1060, "无聊中"),
        (1061, "想静静"),
        (1062, "我太难了"),
        (1063, "一言难尽"),
        (1070, "宝宝认证"),
        (1071, "好运锦鲤"),
        (1201, "水逆退散"),
        (1300, "摸鱼中"),
        (1401, "emo中"),
        (2001, "难得糊涂"),
        (2003, "出去浪"),
        (2006, "爱你"),
        (2012, "肝作业"),
        (2013, "我想开了"),
        (2014, "被掏空"),
        (2015, "去旅行"),
        (2019, "我crash了"),
        (2023, "搬砖中"),
        (2025, "一起元梦"),
        (2026, "求星搭子"),
        (2037, "春日限定"),
    ]
    .into_iter()
    .collect();

    if let Some(&label) = ext_map.get(&ext_status) {
        return label.to_string();
    }
    if let Some(&label) = status_map.get(&status) {
        return label.to_string();
    }
    if status <= 0 && ext_status <= 0 {
        return String::new();
    }
    format!("状态 {} / {}", status, ext_status)
}

fn item(label: &str, value: &str) -> DisplayItem {
    DisplayItem {
        Label: label.to_string(),
        Value: value.to_string(),
    }
}

fn item_if_diff(label: &str, value: &str, diff_to: &str) -> DisplayItem {
    let value = value.trim();
    if value.is_empty() || value == diff_to.trim() {
        DisplayItem {
            Label: String::new(),
            Value: String::new(),
        }
    } else {
        DisplayItem {
            Label: label.to_string(),
            Value: value.to_string(),
        }
    }
}

fn append_section(
    mut sections: Vec<DisplaySection>,
    title: &str,
    grid: &str,
    items: Vec<DisplayItem>,
) -> Vec<DisplaySection> {
    let mut filtered = Vec::new();
    for it in items {
        if !it.Label.trim().is_empty() && !it.Value.trim().is_empty() {
            filtered.push(it);
        }
    }
    if !filtered.is_empty() {
        sections.push(DisplaySection {
            Title: title.to_string(),
            GridTemplate: grid.to_string(),
            Items: filtered,
        });
    }
    sections
}

fn qq_avatar_url(id: &str) -> String {
    format!("https://q1.qlogo.cn/g?b=qq&nk={}&s=640", id)
}

pub async fn user_handler(Query(q): Query<QueryParams>) -> impl IntoResponse {
    let id = q.id.unwrap_or_default().trim().to_string();
    if id.is_empty() {
        return render_html(
            "query/user.html",
            UserResponse {
                User: None,
                Error: Some("缺少用户 ID 参数".to_string()),
            },
        )
        .into_response();
    }
    if !is_digits(&id) {
        return render_html(
            "query/user.html",
            UserResponse {
                User: None,
                Error: Some("无效的用户 ID".to_string()),
            },
        )
        .into_response();
    }

    let reg_year_str = q.reg_year.clone().unwrap_or_default();
    let reg_year = parse_positive_int(&reg_year_str);

    let card_str = q.card.clone().unwrap_or_default();
    let nickname = clamp_text(&q.nickname.unwrap_or_default(), 48);
    let card = clamp_text(&card_str, 48);
    let role_text = humanize_role(&q.role.unwrap_or_default());
    let title = clamp_text(&q.title.unwrap_or_default(), 32);
    let vip_text = humanize_vip(
        &q.is_vip.unwrap_or_default(),
        &q.is_years_vip.unwrap_or_default(),
        parse_positive_int(&q.vip_level.unwrap_or_default()),
    );
    let online_text = humanize_online_status(
        parse_positive_int(&q.online_status.unwrap_or_default()),
        parse_positive_int(&q.online_ext_status.unwrap_or_default()),
    );

    let mut sections = Vec::new();
    sections = append_section(
        sections,
        "身份标识",
        "repeat(2, minmax(0, 1fr))",
        vec![
            item("昵称", &first_non_empty(&[&nickname, &id])),
            item_if_diff("备注", &q.remark.unwrap_or_default(), &nickname),
            item("QID", &q.qid.unwrap_or_default()),
            item("分组名称", &q.category_name.unwrap_or_default()),
            item("分组 ID", &q.category_id.unwrap_or_default()),
        ],
    );

    sections = append_section(
        sections,
        "基础资料",
        "repeat(3, minmax(0, 1fr))",
        vec![
            item("性别", humanize_sex(&q.sex.unwrap_or_default())),
            item(
                "年龄",
                &humanize_int(
                    parse_positive_int(&q.age.unwrap_or_default()),
                    "未知",
                    " 岁",
                ),
            ),
            item(
                "Q龄",
                &first_non_empty(&[&q.qage.unwrap_or_default(), &humanize_q_age(reg_year)]),
            ),
            item("注册年份", &humanize_int(reg_year, "未知", " 年注册")),
            item(
                "QQ 等级",
                &humanize_int(
                    parse_positive_int(&q.qq_level.unwrap_or_default()),
                    "未知",
                    " 级",
                ),
            ),
            item("生日", &q.birthday.unwrap_or_default()),
            item("手机号", &q.phone_num.unwrap_or_default()),
            item("邮箱", &q.email.unwrap_or_default()),
        ],
    );

    sections = append_section(
        sections,
        "群内信息",
        "repeat(3, minmax(0, 1fr))",
        vec![
            item("群名片", &non_empty_or(&card_str, "未设置")),
            item(
                "群等级",
                &non_empty_or(&q.group_level.unwrap_or_default(), "未提供"),
            ),
            item("地区", &non_empty_or(&q.area.unwrap_or_default(), "未提供")),
            item(
                "是否机器人",
                humanize_optional_bool(&q.is_robot.unwrap_or_default()),
            ),
            item(
                "不良记录",
                humanize_optional_bool(&q.unfriendly.unwrap_or_default()),
            ),
        ],
    );

    sections = append_section(
        sections,
        "群内信息",
        "repeat(2, minmax(0, 1fr))",
        vec![
            item("在线状态", &non_empty_or(&online_text, "未提供")),
            item("角色", role_text),
            item("入群时间", &q.join_time.unwrap_or_default()),
            item("最后发言", &q.last_sent_time.unwrap_or_default()),
            item("禁言至", &q.mute_until.unwrap_or_default()),
            item("头衔到期", &q.title_expire_time.unwrap_or_default()),
        ],
    );

    let mut text_blocks = Vec::new();
    let long_nick = clamp_text(&q.long_nick.unwrap_or_default(), QUERY_TEXT_LIMIT);
    if !long_nick.is_empty() {
        text_blocks.push(TextBlock {
            Title: "个性签名".to_string(),
            Value: long_nick,
        });
    }

    let mut badges = Vec::new();
    if role_text != "未提供" {
        badges.push(role_text.to_string());
    }
    if !title.is_empty() {
        badges.push(title);
    }
    if vip_text != "未提供" {
        badges.push(vip_text);
    }
    if !online_text.is_empty() {
        badges.push(online_text);
    }

    let avatar_url = qq_avatar_url(&id);
    let avatar = DEFAULT_IMG_CACHE.download(&avatar_url, None, None).await;

    let data = UserPageData {
        Avatar: avatar,
        DisplayName: first_non_empty(&[&card, &nickname, &id]),
        ID: id,
        Badges: badges,
        Sections: sections,
        TextBlocks: text_blocks,
    };

    render_html(
        "query/user.html",
        UserResponse {
            User: Some(data),
            Error: None,
        },
    )
    .into_response()
}

pub async fn group_handler(Query(_q): Query<QueryParams>) -> impl IntoResponse {
    render_html(
        "query/group.html",
        GroupResponse {
            Group: None,
            Error: Some("Group handler not fully active".to_string()),
        },
    )
    .into_response()
}
