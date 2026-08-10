pub mod bilibili;
pub mod gallery;
pub mod pixiv;
pub mod pjsk;
pub mod query;
pub mod status;

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use minijinja::{path_loader, Environment};
use once_cell::sync::Lazy;
use serde::Serialize;

/// 已知页面模板（用于启动 warmup；单模板失败不影响其它模板）。
const WARMUP_TEMPLATES: &[&str] = &[
    "layout.html",
    "logo.html",
    "bilibili/video.html",
    "gallery/duplicate.html",
    "gallery/tags.html",
    "gallery/images.html",
    "pixiv/illust.html",
    "pjsk/event.html",
    "pjsk/card.html",
    "pjsk/music.html",
    "pjsk/profile.html",
    "pjsk/b30.html",
    "query/user.html",
    "query/group.html",
    "status/zeabur.html",
];

pub static TEMPLATE_ENV: Lazy<Environment<'static>> = Lazy::new(|| {
    let mut env = Environment::new();
    env.set_loader(path_loader("templates"));

    // HTML autoescape for *.html
    env.set_auto_escape_callback(|name| {
        if name.ends_with(".html") || name.ends_with(".htm") {
            minijinja::AutoEscape::Html
        } else {
            minijinja::AutoEscape::None
        }
    });

    for name in WARMUP_TEMPLATES {
        match env.get_template(name) {
            Ok(_) => tracing::debug!(template = %name, "template warmup ok"),
            Err(e) => tracing::error!(template = %name, error = %e, "template warmup failed"),
        }
    }

    env
});

pub use crate::pkg::http_error::format_upstream_http_error;

pub fn render_html<S: Serialize>(template_name: &str, ctx: S) -> impl IntoResponse {
    // Ensure env initialized (warmup logs once)
    let env = &*TEMPLATE_ENV;
    match env.get_template(template_name) {
        Ok(tmpl) => match tmpl.render(ctx) {
            Ok(html) => Html(html).into_response(),
            Err(e) => {
                tracing::error!(template = %template_name, error = %e, "渲染模板失败");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Render error: {e}"),
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(template = %template_name, error = %e, "模板不存在或加载失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Template not found: {e}"),
            )
                .into_response()
        }
    }
}
