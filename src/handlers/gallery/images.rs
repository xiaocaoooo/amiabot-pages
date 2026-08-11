use axum::{extract::Query, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::handlers::gallery::common::{
    build_gallery_file_url, fetch_gallery_images, format_aliases, format_image_created_at,
    format_image_dimensions, gallery_image_downloader, resolve_gallery_from_tags, split_csv_names,
    ImageDetail,
};
use crate::handlers::render_html;

#[derive(Deserialize, Debug)]
pub struct ImagesQuery {
    pub tags: Option<String>,
    /// 可选：显式画廊名（优先于 tags）
    pub gallery: Option<String>,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct GalleryImageCard {
    pub ID: String,
    pub Preview: String,
    pub HasPreview: bool,
    pub Aliases: Vec<String>,
    pub Dimensions: String,
    pub CreatedAt: String,
    pub Name: String,
}

#[derive(Serialize, Clone)]
#[allow(non_snake_case)]
pub struct GalleryImagesPageData {
    pub GalleryName: String,
    pub GalleryAliases: String,
    pub QueryTags: Vec<String>,
    pub Total: i64,
    pub Items: Vec<GalleryImageCard>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct ImagesResponse {
    pub ImagesPage: Option<GalleryImagesPageData>,
    pub Error: Option<String>,
}

pub async fn images_handler(Query(q): Query<ImagesQuery>) -> impl IntoResponse {
    let mut keys = Vec::new();
    if let Some(g) = q
        .gallery
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        keys.push(g.to_string());
    }
    keys.extend(split_csv_names(&q.tags.unwrap_or_default()));
    // dedupe preserving order
    let mut seen = std::collections::HashSet::new();
    keys.retain(|k| seen.insert(k.to_lowercase()));

    if keys.is_empty() {
        return render_html(
            "gallery/images.html",
            ImagesResponse {
                ImagesPage: None,
                Error: Some("缺少画廊名参数（tags 或 gallery）".to_string()),
            },
        );
    }

    let gallery = match resolve_gallery_from_tags(&keys).await {
        Ok(g) => g,
        Err(err) => {
            return render_html(
                "gallery/images.html",
                ImagesResponse {
                    ImagesPage: None,
                    Error: Some(err),
                },
            );
        }
    };

    let images = match fetch_gallery_images(&gallery.id).await {
        Ok(imgs) => images_to_cards(imgs).await,
        Err(err) => {
            return render_html(
                "gallery/images.html",
                ImagesResponse {
                    ImagesPage: None,
                    Error: Some(err),
                },
            );
        }
    };

    let total = images.len() as i64;
    render_html(
        "gallery/images.html",
        ImagesResponse {
            ImagesPage: Some(GalleryImagesPageData {
                GalleryName: gallery.name,
                GalleryAliases: format_aliases(&gallery.aliases),
                QueryTags: keys,
                Total: total,
                Items: images,
            }),
            Error: None,
        },
    )
}

async fn images_to_cards(images: Vec<ImageDetail>) -> Vec<GalleryImageCard> {
    let mut items = Vec::with_capacity(images.len());
    for image in images {
        let preview_url = build_gallery_file_url(&image.id);
        let preview = gallery_image_downloader(&preview_url).await;
        let has_preview = !preview.trim().is_empty();
        let dims = format_image_dimensions(&image);
        let created_at = format_image_created_at(&image);
        let name = image.name.clone().unwrap_or_default();
        items.push(GalleryImageCard {
            ID: image.id,
            Preview: preview,
            HasPreview: has_preview,
            Aliases: image.aliases,
            Dimensions: dims,
            CreatedAt: created_at,
            Name: name,
        });
    }
    items
}
