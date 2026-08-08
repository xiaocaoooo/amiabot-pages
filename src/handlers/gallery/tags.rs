use axum::response::IntoResponse;
use serde::Serialize;
use futures_util::future::join_all;
use crate::handlers::render_html;
use crate::handlers::gallery::common::{
    gallery_image_downloader, build_gallery_preview_url, fetch_gallery_tags,
    fetch_gallery_images_page, GALLERY_TAG_LIST_LIMIT,
};

#[derive(Serialize)]
pub struct GalleryTagCard {
    pub Name: String,
    pub Count: i64,
    pub FirstImageID: i64,
    pub Preview: String,
    pub HasPreview: bool,
}

#[derive(Serialize)]
pub struct GalleryTagsPageData {
    pub TotalTags: usize,
    pub Items: Vec<GalleryTagCard>,
}

#[derive(Serialize)]
pub struct TagsResponse {
    pub TagsPage: Option<GalleryTagsPageData>,
    pub Error: Option<String>,
}

pub async fn tags_handler() -> impl IntoResponse {
    let tags = match fetch_gallery_tags("", GALLERY_TAG_LIST_LIMIT).await {
        Ok(t) => t,
        Err(err) => {
            return render_html("gallery/tags.html", TagsResponse {
                TagsPage: None,
                Error: Some(err),
            });
        }
    };

    let total_tags = tags.len();
    let items = match build_gallery_tag_cards(tags).await {
        Ok(cards) => cards,
        Err(err) => {
            return render_html("gallery/tags.html", TagsResponse {
                TagsPage: None,
                Error: Some(err),
            });
        }
    };

    let data = GalleryTagsPageData {
        TotalTags: total_tags,
        Items: items,
    };

    render_html("gallery/tags.html", TagsResponse {
        TagsPage: Some(data),
        Error: None,
    })
}

async fn build_gallery_tag_cards(tags: Vec<crate::handlers::gallery::common::GalleryTag>) -> Result<Vec<GalleryTagCard>, String> {
    let mut futures = Vec::new();

    for tag in tags {
        futures.push(tokio::spawn(async move {
            let payload = fetch_gallery_images_page(&[tag.name.clone()], 1, 1).await;
            match payload {
                Ok(p) => {
                    let mut card = GalleryTagCard {
                        Name: tag.name,
                        Count: p.total,
                        FirstImageID: 0,
                        Preview: String::new(),
                        HasPreview: false,
                    };
                    if !p.items.is_empty() {
                        let first = &p.items[0];
                        card.FirstImageID = first.image.id;
                        let preview_url = build_gallery_preview_url(first);
                        let preview = gallery_image_downloader(&preview_url).await;
                        card.HasPreview = !preview.trim().is_empty();
                        card.Preview = preview;
                    }
                    Ok(card)
                }
                Err(e) => Err(format!("加载标签 #{} 首图失败: {}", tag.name, e)),
            }
        }));
    }

    let results = join_all(futures).await;
    let mut cards = Vec::new();
    for res in results {
        match res {
            Ok(Ok(card)) => cards.push(card),
            Ok(Err(err)) => return Err(err),
            Err(e) => return Err(format!("Task panic: {}", e)),
        }
    }
    Ok(cards)
}
