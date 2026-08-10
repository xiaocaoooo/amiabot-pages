/// 标准化上游 HTTP 错误文案，附带截断后的 body，便于日志与响应排查。
pub fn format_upstream_http_error(
    context: &str,
    status: impl std::fmt::Display,
    body: impl AsRef<str>,
) -> String {
    let body = body.as_ref().trim();
    if body.is_empty() {
        return format!("{context}: HTTP {status}");
    }

    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let max_chars = 1024usize;
    let truncated = if compact.chars().count() > max_chars {
        let s: String = compact.chars().take(max_chars).collect();
        format!("{s}…")
    } else {
        compact
    };
    format!("{context}: HTTP {status}; body={truncated}")
}
