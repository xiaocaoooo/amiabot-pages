pub mod bilibili;
pub mod gallery;
pub mod pixiv;
pub mod pjsk;
pub mod query;
pub mod status;

use axum::response::{Html, IntoResponse};
use axum::http::StatusCode;
use minijinja::Environment;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::path::Path;

pub static TEMPLATE_ENV: Lazy<Environment<'static>> = Lazy::new(|| {
    let mut env = Environment::new();
    
    // Custom template loader
    let load_templates = |env: &mut Environment<'static>| -> Result<(), String> {
        let dir = Path::new("templates");
        if !dir.exists() {
            return Err("Templates directory not found".to_string());
        }

        let paths = [
            "templates/layout.html",
            "templates/logo.html",
            "templates/bilibili/video.html",
            "templates/gallery/duplicate.html",
            "templates/gallery/tags.html",
            "templates/gallery/images.html",
            "templates/pixiv/illust.html",
            "templates/pjsk/event.html",
            "templates/pjsk/card.html",
            "templates/pjsk/music.html",
            "templates/pjsk/profile.html",
            "templates/pjsk/b30.html",
            "templates/query/user.html",
            "templates/query/group.html",
            "templates/status/zeabur.html",
        ];

        // First, read layout.html to split layout/start and layout/end dynamically!
        let layout_content = std::fs::read_to_string("templates/layout.html")
            .map_err(|e| format!("Failed to read layout.html: {}", e))?;

        // We can extract layout/start and layout/end!
        // Start block is from `{{ define "layout/start" }}` to `{{ end }}` (the first `{{ end }}`)
        // End block is from `{{ define "layout/end" }}` to `{{ end }}`
        let (layout_start, layout_end) = extract_layout_parts(&layout_content);

        env.add_template_owned("layout/start".to_string(), preprocess_go_template(&layout_start, "layout/start"))
            .map_err(|e| format!("Failed to register layout/start: {}", e))?;
        env.add_template_owned("layout/end".to_string(), preprocess_go_template(&layout_end, "layout/end"))
            .map_err(|e| format!("Failed to register layout/end: {}", e))?;

        for path in &paths {
            let name = path.strip_prefix("templates/").unwrap_or(path);
            if name == "layout.html" {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                let processed = preprocess_go_template(&content, name);
                env.add_template_owned(name.to_string(), processed)
                    .map_err(|e| format!("Failed to load {}: {}", name, e))?;
            }
        }
        Ok(())
    };

    if let Err(e) = load_templates(&mut env) {
        tracing::error!(error = %e, "加载模板失败");
    }
    
    env
});

fn extract_layout_parts(layout: &str) -> (String, String) {
    let mut start = String::new();
    let mut end = String::new();

    // Splitting Go templates manually:
    // Layout file layout.html has layout/start defined first and then layout/end
    if let Some(start_def_idx) = layout.find("{{ define \"layout/start\" }}") {
        let content_after = &layout[start_def_idx + "{{ define \"layout/start\" }}".len()..];
        // Find first "{{ end }}"
        if let Some(end_def_idx) = content_after.find("{{ define \"layout/end\" }}") {
            // Check for "{{ end }}" right before defined layout/end
            let mut start_part = &content_after[..end_def_idx];
            if let Some(last_end_idx) = start_part.rfind("{{ end }}") {
                start_part = &start_part[..last_end_idx];
            }
            start = start_part.to_string();

            let end_content_after = &layout[start_def_idx + "{{ define \"layout/start\" }}".len() + end_def_idx + "{{ define \"layout/end\" }}".len()..];
            // Find trailing "{{ end }}"
            let mut end_part = end_content_after.to_string();
            if let Some(last_end_idx) = end_part.rfind("{{ end }}") {
                end_part = end_part[..last_end_idx].to_string();
            }
            end = end_part;
        }
    }

    if start.is_empty() {
        start = layout.to_string();
    }
    if end.is_empty() {
        end = layout.to_string();
    }

    (start, end)
}

fn preprocess_go_template(content: &str, name: &str) -> String {
    let mut s = content.to_string();

    // Replace include directives
    s = s.replace("{{ template \"layout/start\" . }}", "{% include \"layout/start\" %}");
    s = s.replace("{{ template \"layout/end\" . }}", "{% include \"layout/end\" %}");
    s = s.replace("{{ template \"logo\" . }}", "{% include \"logo.html\" %}");
    s = s.replace("{{ template \"logo\" }}", "{% include \"logo.html\" %}");

    // Convert Go template actions to Jinja2 syntax
    let translated = translate_go_syntax_to_jinja(&s);
    
    // Fix `{% end %}` using stack
    let mut final_result = String::new();
    let mut stack = Vec::new();
    let mut rest = &translated[..];
    
    while let Some(start_idx) = rest.find("{%") {
        final_result.push_str(&rest[..start_idx]);
        let next_part = &rest[start_idx..];
        
        if let Some(end_idx) = next_part.find("%}") {
            let tag_content = &next_part[2..end_idx];
            let trimmed = tag_content.trim();
            
            let mut output_tag = next_part[..end_idx + 2].to_string();
            
            if trimmed.starts_with("if") {
                stack.push("if");
            } else if trimmed.starts_with("for") {
                stack.push("for");
            } else if trimmed.starts_with("block") {
                stack.push("block");
            } else if trimmed == "end" {
                if let Some(top) = stack.pop() {
                    output_tag = format!("{{% end{} %}}", top);
                } else {
                    output_tag = "{% endif %}".to_string(); // fallback
                }
            }
            
            final_result.push_str(&output_tag);
            rest = &next_part[end_idx + 2..];
        } else {
            final_result.push_str("{%");
            rest = &next_part[2..];
        }
    }
    final_result.push_str(rest);
    
    // Fix Go's `index .B30.Scores 0` style indexing
    let mut res = final_result;
    res = res.replace("index ", "");
    res
}

fn translate_go_syntax_to_jinja(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second '{'
            
            // Read until '}' and '}'
            let mut action = String::new();
            let mut found_end = false;
            while let Some(&next_ch) = chars.peek() {
                if next_ch == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        found_end = true;
                        break;
                    } else {
                        action.push('}');
                    }
                } else {
                    action.push(chars.next().unwrap());
                }
            }
            
            if found_end {
                let parsed = translate_action(&action);
                result.push_str(&parsed);
            } else {
                result.push_str("{{");
                result.push_str(&action);
            }
        } else {
            result.push(ch);
        }
    }
    
    result
}

fn translate_action(action: &str) -> String {
    let raw_trimmed = action.trim();
    if raw_trimmed.is_empty() {
        return "{{}}".to_string();
    }

    let parts: Vec<&str> = raw_trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return "{{}}".to_string();
    }

    let keyword = parts[0];
    match keyword {
        "define" => {
            let name = parts.get(1).unwrap_or(&"\"\"").trim_matches('"');
            format!("{{% block {} %}}", name.replace('/', "_"))
        }
        "template" => {
            let name = parts.get(1).unwrap_or(&"\"\"").trim_matches('"');
            let j_name = if name == "logo" {
                "logo.html"
            } else {
                name
            };
            format!("{{% include \"{}\" %}}", j_name)
        }
        "if" => {
            let cond = parts[1..].join(" ");
            let cond = cond.replace('.', ""); // e.g. .Error -> Error
            format!("{{% if {} %}}", cond)
        }
        "else" => {
            if parts.len() > 1 && parts[1] == "if" {
                let cond = parts[2..].join(" ");
                let cond = cond.replace('.', "");
                format!("{{% elif {} %}}", cond)
            } else {
                "{% else %}".to_string()
            }
        }
        "range" => {
            let target = parts.last().unwrap_or(&"").trim_start_matches('.');
            let target = if target.starts_with('.') { &target[1..] } else { target };
            
            let mut var = "item";
            if parts.len() >= 4 {
                if parts[1].starts_with('$') {
                    var = parts[2].trim_start_matches('$').trim_end_matches(',');
                    if var.is_empty() {
                        var = parts[1].trim_start_matches('$').trim_end_matches(',');
                    }
                }
            }
            let var = match target {
                "Pages" => "page",
                "Scores" => "score",
                "Cards" => "card",
                "Vocalists" => "vocalist",
                "Difficulties" => "difficulty",
                "Events" => "event",
                "Items" => "item",
                _ => "item",
            };
            format!("{{% for {} in {} %}}", var, target)
        }
        "end" => {
            "{% end %}".to_string()
        }
        _ => {
            let expr = raw_trimmed.trim_start_matches('.');
            let expr = expr.replace('$', "");
            format!("{{{{ {} }}}}", expr)
        }
    }
}


pub use crate::pkg::http_error::format_upstream_http_error;

pub fn render_html<S: Serialize>(template_name: &str, ctx: S) -> impl IntoResponse {
    match TEMPLATE_ENV.get_template(template_name) {
        Ok(tmpl) => match tmpl.render(ctx) {
            Ok(html) => Html(html).into_response(),
            Err(e) => {
                tracing::error!(error = %e, "渲染模板失败");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Render error: {}", e)).into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "模板不存在");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Template not found: {}", e)).into_response()
        }
    }
}
