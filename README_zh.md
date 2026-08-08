# amiabot-pages (Rust 重构版)

本项目是基于 **Rust (Axum + MiniJinja + Tokio)** 重构的高性能 HTML 卡片渲染与媒体代理服务，无缝继承并完全兼容原 Go 版本的全部行为（模板、缓存、多源回退与参数注入）。

## 核心特性

- **极致性能**: 采用 Rust 现代 Web 框架 `axum` 与超轻量级 `minijinja` 模板引擎，渲染延迟低至微秒级。
- **参数注入中间件**: 通过 Valkey/Redis 解析 `param_id` 自动合并与重构复杂的 Query 请求，对后面的路由层完全透明。
- **智能媒体代理**: 代理并缓存 B站、Pixiv、PJSK 的网络资源，提供在内存中解压 **Pixiv Ugoira (ZIP) 并无缝转换为标准 GIF 动图** 的高级处理能力。
- **PJSK 资源多源回退**: 按照 `snowy → uni → haruki` 优先级并发/顺序回退获取图片资源，重试 2 轮确保渲染可靠。
- **MasterData 增量同步**: 在服务启动时极速通过 GitHub Commit API 判断变更，多线程并发拉取 Project Sekai 静态数据。
- **内置 Swagger UI**: 原生集成 Swagger UI 交互式 API 测试控制台，直接访问 `/swagger-ui/` 即可调试。

---

## 环境变量配置

| 变量名 | 默认值 | 说明 |
|---|---|---|
| `PORT` | `8080` | 服务监听端口 |
| `SEKAI_ASSET` | `snowy,uni,haruki` | 世界计划资源服务器的优先级配置 (逗号分隔) |
| `VALKEY_ADDR` | 无 | Valkey/Redis 服务器连接地址，配置后即可启用 `param_id` 注入 |
| `IMAGE_CACHE_MAX_SIZE` | `512` | 静态图片缓存的最大容量限制 (MB) |
| `PJSK_PROFILE_BASEURL` | 无 | 上游 PJSK Profile/B30 的 API 基础端点 |

---

## 本地运行

确保本地装有 Rust 编译器及 Cargo 工具 (Arch Linux 用户直接运行 `pacman -S rust` 即可)。

```bash
cargo build --release
cargo run
```

运行后访问 `http://localhost:8080/swagger-ui/` 即可直接在网页上发起请求测试各个渲染接口！

---

## Docker 容器化支持

项目提供了生产级别的多阶段构建 Docker 镜像，完美支持依赖层的 Layer 缓存：

```bash
# 编译并打包本地镜像
docker build -t amiabot-pages .

# 一键 Compose 编排启动
docker compose up -d
```

---

由 Parallel SEKAI 精心设计，由 暁山瑞希 (Codex 重构版) 喵声诚制！
