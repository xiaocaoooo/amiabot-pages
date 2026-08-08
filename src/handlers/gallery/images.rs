use axum::{
    extract::Query,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::handlers::render_html;
use crate::handlers::gallery::common::{
    gallery_image_downloader, build_gallery_preview_url, split_gallery_tags,
    fetch_all_gallery_images, format_gallery_dimensions, format_gallery_created_at,
};

#[derive(Deserialize, Debug)]
pub struct ImagesQuery {
    pub tags: Option<String>,
}

#[derive(Serialize)]
pub struct GalleryImageCard {
    pub ID: i64,
    pub Preview: String,
    pub HasPreview: bool,
    pub Tags: Vec<String>,
    pub Dimensions: String,
    pub CreatedAt: String,
}

#[derive(Serialize)]
pub struct GalleryImagesPageData {
    pub QueryTags: Vec<String>,
    pub Total: i64,
    pub Items: Vec<GalleryImageCard>,
}

#[derive(Serialize)]
pub struct ImagesResponse {
    pub ImagesPage: Option<GalleryImagesPageData>,
    pub Error: Option<String>,
}

pub async fn images_handler(Query(q): Query<ImagesQuery>) -> impl IntoResponse {
    let tags = split_gallery_tags(&q.tags.unwrap_or_default());
    if tags.is_empty() {
        return render_html("gallery/images.html", ImagesResponse {
            ImagesPage: None,
            Error: Some("缺少标签参数，无法生成图片列表页".to_string()),
        });
    }

    let images = match fetch_all_gallery_images(&tags).await {
        Ok(imgs) => images_to_cards(imgs).await,
        Err(err) => {
            return render_html("gallery/images.html", ImagesResponse {
                ImagesPage: None,
                Error: Some(err),
            });
        }
    };

    let total = images.len() as i64;
    let data = GalleryImagesPageData {
        QueryTags: tags,
        Total: total,
        Items: images,
    };

    render_html("gallery/images.html", ImagesResponse {
        ImagesPage: Some(data),
        Error: None,
    })
}

async fn images_to_cards(images: Vec<crate::handlers::gallery::common::GalleryImageWithTags>) -> Vec<GalleryImageCard> {
    let mut items = Vec::new();
    for image in images {
        let preview_url = build_gallery_preview_url(&image);
        let preview = gallery_image_downloader(&preview_url).await;
        let has_preview = !preview.trim().is_empty();
        
        let tags: Vec<String> = image.tags.iter().map(|t| t.name.clone()).collect();
        let dims = format_gallery_dimensions(&image);
        let created_at = format_gallery_created_at(&image);

        items.push(GalleryImageCard {
            ID: image.image.id,
            Preview: preview,
            HasPreview: has_preview,
            Tags: tags,
            Dimensions: dims,
            CreatedAt: created_at,
        });
    }
    items
}
