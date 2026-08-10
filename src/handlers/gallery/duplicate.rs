use axum::{
    extract::Query,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::handlers::render_html;
use crate::handlers::gallery::common::{
    gallery_image_downloader, build_gallery_render_url, split_gallery_tags,
};

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[allow(dead_code)]
pub struct DuplicateQuery {
    pub current_image_url: Option<String>,
    pub duplicate_id: Option<String>,
    pub current_tags: Option<String>,
    pub existing_tags: Option<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DuplicatePageData {
    pub CurrentImage: String,
    pub ExistingImage: String,
    pub DuplicateID: String,
    pub CurrentTags: Vec<String>,
    pub ExistingTags: Vec<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct DuplicateResponse {
    pub Duplicate: Option<DuplicatePageData>,
    pub Error: Option<String>,
}

pub async fn duplicate_handler(Query(q): Query<DuplicateQuery>) -> impl IntoResponse {
    let current_url = q.current_image_url.unwrap_or_default().trim().to_string();
    let duplicate_id = q.duplicate_id.unwrap_or_default().trim().to_string();

    if current_url.is_empty() || duplicate_id.is_empty() {
        return render_html("gallery/duplicate.html", DuplicateResponse {
            Duplicate: None,
            Error: Some("缺少必要参数，无法生成重复图片对比页".to_string()),
        });
    }

    let parsed_id: i64 = match duplicate_id.parse() {
        Ok(id) if id > 0 => id,
        _ => {
            return render_html("gallery/duplicate.html", DuplicateResponse {
                Duplicate: None,
                Error: Some("重复图片地址无效，无法生成对比页".to_string()),
            });
        }
    };

    let existing_url = build_gallery_render_url(parsed_id);
    if existing_url.is_empty() {
        return render_html("gallery/duplicate.html", DuplicateResponse {
            Duplicate: None,
            Error: Some("重复图片地址无效，无法生成对比页".to_string()),
        });
    }

    let current_image = gallery_image_downloader(&current_url).await;
    let existing_image = gallery_image_downloader(&existing_url).await;

    if current_image.is_empty() || existing_image.is_empty() {
        return render_html("gallery/duplicate.html", DuplicateResponse {
            Duplicate: None,
            Error: Some("图片加载失败，请检查图片地址是否可访问".to_string()),
        });
    }

    let data = DuplicatePageData {
        CurrentImage: current_image,
        ExistingImage: existing_image,
        DuplicateID: duplicate_id,
        CurrentTags: split_gallery_tags(&q.current_tags.unwrap_or_default()),
        ExistingTags: split_gallery_tags(&q.existing_tags.unwrap_or_default()),
    };

    render_html("gallery/duplicate.html", DuplicateResponse {
        Duplicate: Some(data),
        Error: None,
    })
}
