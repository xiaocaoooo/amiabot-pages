use axum::{extract::Query, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::handlers::gallery::common::{
    build_gallery_file_url, fetch_galleries, fetch_gallery_images, format_aliases,
    gallery_image_downloader, GalleryDetail,
};
use crate::handlers::render_html;

#[derive(Deserialize, Debug)]
pub struct TagsQuery {}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct GalleryCard {
    pub ID: String,
    pub Name: String,
    pub Aliases: String,
    pub Preview: String,
    pub HasPreview: bool,
    pub ImageCount: i64,
    pub FirstImageID: String,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct GalleryListPageData {
    pub TotalGalleries: i64,
    pub Items: Vec<GalleryCard>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct TagsResponse {
    pub TagsPage: Option<GalleryListPageData>,
    pub Error: Option<String>,
}

pub async fn tags_handler(Query(_q): Query<TagsQuery>) -> impl IntoResponse {
    let galleries = match fetch_galleries(None).await {
        Ok(g) => g,
        Err(err) => {
            return render_html(
                "gallery/tags.html",
                TagsResponse {
                    TagsPage: None,
                    Error: Some(err),
                },
            );
        }
    };

    let items = match build_gallery_cards(galleries).await {
        Ok(items) => items,
        Err(err) => {
            return render_html(
                "gallery/tags.html",
                TagsResponse {
                    TagsPage: None,
                    Error: Some(err),
                },
            );
        }
    };

    let total = items.len() as i64;
    render_html(
        "gallery/tags.html",
        TagsResponse {
            TagsPage: Some(GalleryListPageData {
                TotalGalleries: total,
                Items: items,
            }),
            Error: None,
        },
    )
}

async fn build_gallery_cards(galleries: Vec<GalleryDetail>) -> Result<Vec<GalleryCard>, String> {
    let mut items = Vec::with_capacity(galleries.len());
    for g in galleries {
        let images = fetch_gallery_images(&g.id).await.unwrap_or_default();
        let image_count = images.len() as i64;
        let (preview, first_id) = if let Some(first) = images.first() {
            let url = build_gallery_file_url(&first.id);
            let preview = gallery_image_downloader(&url).await;
            (preview, first.id.clone())
        } else {
            (String::new(), String::new())
        };
        let has_preview = !preview.trim().is_empty();
        items.push(GalleryCard {
            ID: g.id,
            Name: g.name,
            Aliases: format_aliases(&g.aliases),
            Preview: preview,
            HasPreview: has_preview,
            ImageCount: image_count,
            FirstImageID: first_id,
        });
    }
    Ok(items)
}
