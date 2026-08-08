use axum::{
    routing::{get, post},
    Router,
    middleware,
};
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod pkg;
mod handlers;

use crate::pkg::imgcache::DEFAULT_IMG_CACHE;
use crate::pkg::paramid::ParamIDMiddleware;

use crate::handlers::bilibili::video_handler;
use crate::handlers::gallery::duplicate::duplicate_handler;
use crate::handlers::gallery::images_handler;
use crate::handlers::gallery::tags::tags_handler;
use crate::handlers::pixiv::{
    illust_info_handler, illust_media_handler, pixiv_image_proxy_handler, pixiv_ugoira_gif_handler,
};
use crate::handlers::pjsk::assets::{init_master_data, master_data_handler};
use crate::handlers::pjsk::asset_binary::asset_binary_handler;
use crate::handlers::pjsk::b30::b30_handler;
use crate::handlers::pjsk::card::card_handler;
use crate::handlers::pjsk::event::{event_handler, current_event_handler};
use crate::handlers::pjsk::music::music_handler;
use crate::handlers::pjsk::profile::{profile_handler, profile_raw_handler};
use crate::handlers::query::{user_handler, group_handler};
use crate::handlers::status::zeabur_page_handler;

#[derive(OpenApi)]
#[openapi(
    paths(
        // We can register paths here for API documentation
    ),
    info(
        title = "AmiaBot Pages API",
        version = "1.0.0",
        description = "基于 Axum & MiniJinja 的高性能渲染卡片页面服务，由 暁山瑞希 (Codex 重构版) 倾情提供喵！"
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    let paramid_mw = match ParamIDMiddleware::new_from_env() {
        Ok(mw) => mw,
        Err(e) => {
            eprintln!("初始化 param_id 中间件失败: {}", e);
            std::process::exit(1);
        }
    };

    // Build routes
    let mut api_routes = Router::new()
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))

        // Health check
        .route("/health", get(|| async { axum::Json(serde_json::json!({"status": "ok"})) }))
        
        // Status routes
        .route("/status/zeabur", get(zeabur_page_handler))
        
        // Bilibili routes
        .route("/bilibili/video", get(video_handler))
        
        // Gallery routes
        .route("/gallery/duplicate", get(duplicate_handler))
        .route("/gallery/tags", get(tags_handler))
        .route("/gallery/images", get(images_handler))

        // Pixiv routes
        .route("/pixiv/illust/info", get(illust_info_handler))
        .route("/pixiv/illust/media", get(illust_media_handler))
        .route("/pixiv/image", get(pixiv_image_proxy_handler))
        .route("/pixiv/ugoira/gif", get(pixiv_ugoira_gif_handler))

        // PJSK routes
        .route("/pjsk/event", get(event_handler))
        .route("/pjsk/event/current", get(current_event_handler))
        .route("/pjsk/card", get(card_handler))
        .route("/pjsk/music", get(music_handler))
        .route("/pjsk/profile", get(profile_handler))
        .route("/pjsk/profile/raw", get(profile_raw_handler))
        .route("/pjsk/b30", get(b30_handler))
        .route("/pjsk/masterdata/*path", get(master_data_handler))
        .route("/pjsk/assets/:label", get(asset_binary_handler))

        // Query routes
        .route("/query/user", get(user_handler))
        .route("/query/group", get(group_handler));

    // Optional: inject param_id middleware
    if paramid_mw.is_enabled() {
        println!("[paramid] 已启用 param_id 参数注入中间件");
        let paramid_mw_clone = paramid_mw.clone();
        api_routes = api_routes.layer(middleware::from_fn(move |req, next| {
            let mw = paramid_mw_clone.clone();
            async move { mw.handle(req, next).await }
        }));
    }

    // Initialize PJSK masterdata download & image cache
    init_master_data().await;

    DEFAULT_IMG_CACHE.load_index().await;
    DEFAULT_IMG_CACHE.clone().start_cleanup_ticker(Duration::from_secs(600));

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, api_routes).await.unwrap();
}
