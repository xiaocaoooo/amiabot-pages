use axum::{extract::Query, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::handlers::gallery::common::{build_gallery_file_url, gallery_image_downloader};
use crate::handlers::render_html;

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct DuplicateQuery {
    pub current_image_url: Option<String>,
    /// 库中已存在图片 UUID
    pub duplicate_id: Option<String>,
    /// 画廊名（展示用）
    pub gallery: Option<String>,
    // 兼容旧参数（忽略）
    pub current_tags: Option<String>,
    pub existing_tags: Option<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct DuplicatePageData {
    pub DuplicateID: String,
    pub GalleryName: String,
    pub CurrentImage: String,
    pub ExistingImage: String,
    pub HasCurrentImage: bool,
    pub HasExistingImage: bool,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DuplicateResponse {
    pub Duplicate: Option<DuplicatePageData>,
    pub Error: Option<String>,
}

pub async fn duplicate_handler(Query(q): Query<DuplicateQuery>) -> impl IntoResponse {
    let current_url = q.current_image_url.unwrap_or_default().trim().to_string();
    if current_url.is_empty() {
        return render_html(
            "gallery/duplicate.html",
            DuplicateResponse {
                Duplicate: None,
                Error: Some("缺少 current_image_url 参数".to_string()),
            },
        );
    }

    let duplicate_id = q.duplicate_id.unwrap_or_default().trim().to_string();
    if duplicate_id.is_empty() {
        return render_html(
            "gallery/duplicate.html",
            DuplicateResponse {
                Duplicate: None,
                Error: Some("缺少 duplicate_id 参数".to_string()),
            },
        );
    }

    // 粗校验 UUID 形态（8-4-4-4-12），不引入 uuid crate
    if !looks_like_uuid(&duplicate_id) {
        return render_html(
            "gallery/duplicate.html",
            DuplicateResponse {
                Duplicate: None,
                Error: Some(format!(
                    "无效的 duplicate_id（需要 UUID）：{}",
                    duplicate_id
                )),
            },
        );
    }

    let existing_url = build_gallery_file_url(&duplicate_id);
    if existing_url.is_empty() {
        return render_html(
            "gallery/duplicate.html",
            DuplicateResponse {
                Duplicate: None,
                Error: Some("未配置 GALLERY_SERVER，无法拼接已收录图片地址".to_string()),
            },
        );
    }

    let current_image = gallery_image_downloader(&current_url).await;
    let existing_image = gallery_image_downloader(&existing_url).await;
    if current_image.trim().is_empty() || existing_image.trim().is_empty() {
        return render_html(
            "gallery/duplicate.html",
            DuplicateResponse {
                Duplicate: None,
                Error: Some(
                    "对比图下载失败，请检查 current_image_url 与 gallery 文件服务".to_string(),
                ),
            },
        );
    }

    let gallery_name = q.gallery.unwrap_or_default().trim().to_string();

    render_html(
        "gallery/duplicate.html",
        DuplicateResponse {
            Duplicate: Some(DuplicatePageData {
                DuplicateID: duplicate_id,
                GalleryName: gallery_name,
                CurrentImage: current_image,
                ExistingImage: existing_image,
                HasCurrentImage: true,
                HasExistingImage: true,
            }),
            Error: None,
        },
    )
}

fn looks_like_uuid(s: &str) -> bool {
    let s = s.trim();
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lens = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(lens.iter())
        .all(|(p, &n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}
