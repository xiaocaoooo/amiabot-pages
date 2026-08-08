# amiabot-pages (Rust Rewrite)

An ultra-high performance HTML card rendering and media proxy service rewritten entirely in **Rust (Axum + MiniJinja + Tokio)**, designed to provide pixel-perfect image preview pages for AmiaBot plugins.

## Features

- **Blazing Fast Rendering**: Replaced Go Gin with Rust `axum` and `minijinja` for incredibly low response latency.
- **Param Injection Middleware**: Dynamic query parameter reconstruction and merge via Valkey/Redis (`param_id` parameter injection).
- **Intelligent Media Proxying**: Streamlined caching proxy for Pixiv/Bilibili/PJSK assets including automatic **Pixiv Ugoira (ZIP) to GIF** animation conversion in memory.
- **Multi-Source Asset Fallbacks**: Order-based prioritized asset recovery with double-round retry (snowy → uni → haruki).
- **Incremental Synchronized MasterData**: Instant GitHub Commit API verification to perform asynchronous concurrent MasterData sync for Project Sekai servers.
- **Embedded Swagger UI**: Interactive API testing playground natively exposed at `/swagger-ui/`.

---

## Environment Configuration

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8080` | HTTP listening port |
| `SEKAI_ASSET` | `snowy,uni,haruki` | Prioritized sources for PJSK assets |
| `VALKEY_ADDR` | None | Valkey/Redis address to enable `param_id` |
| `IMAGE_CACHE_MAX_SIZE` | `512` | Image cache folder disk budget limit (MB) |
| `PJSK_PROFILE_BASEURL` | None | Upstream suite-api profile URL |

---

## Local Development

Ensure Rust `1.97.1` or later is installed (Arch Linux users can simply do `pacman -S rust`).

```bash
cargo build --release
cargo run
```

Access **Swagger UI** at: `http://localhost:8080/swagger-ui/` for full endpoint interactive documentation.

---

## Docker Support

We provide dynamic multi-stage builds caching Cargo dependency layers for immediate rebuild performance.

```bash
# Build the production image
docker build -t amiabot-pages .

# Run with compose
docker compose up -d
```

---

## License

Under MIT License. Designed by Parallel SEKAI & built with 💖 by 暁山瑞希.
