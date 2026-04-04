# AGENTS.md — amiabot-pages

> 面向 AI 编程代理的项目指南。请在修改本项目代码前通读本文档。

## 1. 项目概述

amiabot-pages 是一个基于 Go + Gin 的 **HTML 页面渲染服务**，为 AmiaBot 项目生成信息卡片页面，供截图插件抓取后发送给用户。

支持四类页面：
- **Bilibili** — B站视频信息卡片（封面、UP主、播放/点赞/投币统计、分P列表）
- **Pixiv** — 插画信息页 + 媒体代理（含 ugoira 动图转 GIF）
- **PJSK** — 世界计划（Project Sekai）活动页、卡面页、音乐页、玩家 Profile 页
- **Status** — Zeabur 服务器/项目/服务状态展示

本服务独立部署，通过 HTTP 提供页面。AmiaBot 外置插件拼接页面 URL → 调用截图插件截图 → 发送图片。

## 2. 技术栈速查

| 项目 | 版本/说明 |
|------|-----------|
| Go | 1.25.6 |
| gin-gonic/gin | v1.11.0 — HTTP 框架 |
| html/template | Go 标准库 — HTML 模板渲染 |
| go-redis/v9 | v9.18.0 — Valkey/Redis 客户端（param_id 参数注入） |
| Docker | 多阶段构建（golang:1.25-alpine → alpine:3.20） |
| GitHub Actions | push main → 构建测试 → Docker Hub 推送 |
| 测试辅助 | alicebob/miniredis/v2 v2.37.0（Redis mock） |

## 3. 目录结构

```
amiabot-pages/
├── main.go                          # 入口：路由注册、中间件初始化、服务启动
├── go.mod / go.sum                  # Go 模块定义与依赖锁定
├── Dockerfile                       # 多阶段 Docker 构建（golang → alpine）
├── docker-compose.yml               # 本地容器编排
├── .dockerignore                    # 构建排除项
├── .github/workflows/
│   └── docker-build.yml             # CI: push main → 构建测试 → Docker 推送
├── handlers/                        # HTTP handler 层（按功能域分包）
│   ├── bilibili/
│   │   └── video.go                 # B站视频卡片 handler
│   ├── pixiv/
│   │   ├── client.go                # Pixiv API 封装（OAuth、token 管理）
│   │   ├── illust.go                # 插画信息页 handler
│   │   ├── illust_test.go           # 测试
│   │   ├── media.go                 # 媒体代理 handler（图片/ugoira GIF）
│   │   └── media_test.go            # 测试
│   ├── pjsk/
│   │   ├── assets.go                # PJSK JSON 资源下载与缓存（MasterData）
│   │   ├── asset_source.go          # 图片资源多源解析（snowy/uni/haruki）
│   │   ├── asset_binary.go          # 资源二进制代理接口
│   │   ├── event.go                 # 活动页面 / 当前活动跳转
│   │   ├── card.go                  # 卡面页面
│   │   ├── music.go                 # 音乐页面
│   │   ├── profile.go               # 玩家 Profile 页（最大文件，~36KB）
│   │   └── profile_test.go          # 测试
│   └── status/
│       └── zeabur.go                # Zeabur 状态页 handler
├── templates/                       # Go HTML 模板
│   ├── layout.html                  # 全局布局（含 CSS 变量，Material Design 3 风格）
│   ├── logo.html                    # Logo 组件
│   ├── bilibili/video.html
│   ├── pixiv/illust.html
│   ├── pjsk/event.html
│   ├── pjsk/card.html
│   ├── pjsk/music.html
│   ├── pjsk/profile.html
│   └── status/zeabur.html
├── static/pjsk/card/                # 静态素材（卡框、属性图标、星星）
├── pkg/                             # 通用工具包
│   ├── imgcache/
│   │   └── imgcache.go              # 图片下载+dataURL 缓存管理器（SHA256 索引）
│   └── paramid/
│       ├── middleware.go             # param_id → Valkey 参数注入中间件
│       └── middleware_test.go
├── cache/                           # 运行时缓存目录（gitignored）
```

## 4. 核心模块详解

### 4.1 main.go — 入口

- 创建 Gin 引擎（`gin.Default()`）
- 初始化 `paramid` 中件间（从 Valkey 读取 param_id 对应参数 JSON，合并到 query）
- 通过 `r.LoadHTMLFiles()` 显式加载所有 HTML 模板
- 注册四个路由组：`/bilibili`、`/pixiv`、`/pjsk`、`/status`（均挂载 paramid 中间件）
- 额外注册 `/health` 健康检查端点
- 启动 PJSK MasterData 后台拉取（`pjsk.InitMasterData()`）
- 初始化图片缓存索引，启动每 10 分钟的定期清理
- 监听 `PORT` 环境变量（默认 8080）

### 4.2 handlers/bilibili/ — B站视频卡片

- 调用 `api.bilibili.com` 获取视频信息
- 将封面和 UP 主头像转换为 base64 dataURL 嵌入 HTML
- 数字格式化：万/亿（如 `123.4万`）
- 渲染 `templates/bilibili/video.html`

### 4.3 handlers/pixiv/ — Pixiv 插画

- **client.go** — 封装 Pixiv App API，管理 OAuth token（access_token / refresh_token）
- **illust.go** — 渲染插画信息页（标签、作者、统计数据）
- **media.go** — 媒体代理：
  - `/image` — 代理 Pixiv 图片（带 Referer）
  - `/ugoira/gif` — 下载 ugoira ZIP → 合成为 GIF 返回

### 4.4 handlers/pjsk/ — 世界计划

- **assets.go** — 从 GitHub 拉取 MasterData JSON，按 commit SHA 增量更新（无变化不下载）
- **asset_source.go** — 图片资源三源回退：snowy → uni → haruki（顺序由 `SEKAI_ASSET` 配置），2 轮重试
- **asset_binary.go** — 资源二进制代理接口（`/pjsk/assets/:label`），设置 `Cache-Control: public, max-age=31536000, immutable`
- **event.go** — 活动页面 + `/event/current` 当前活动跳转
- **card.go** — 卡面页面（使用 `static/pjsk/card/` 中的卡框素材）
- **music.go** — 音乐页面
- **profile.go** — 玩家 Profile 页（本项目最大的 handler 文件，~36KB），调用上游 API 获取玩家数据

### 4.5 handlers/status/ — Zeabur 状态

- 调用 Zeabur GraphQL API 查询服务器/项目/服务状态
- 渲染 `templates/status/zeabur.html`

### 4.6 pkg/imgcache/ — 图片缓存

- 下载远程图片 → SHA256 哈希文件名 → base64 dataURL 格式缓存到本地
- 支持 TTL 和容量上限清理（`IMAGE_CACHE_MAX_SIZE`，默认 512MB）
- 启动时加载已有索引，运行时每 10 分钟清理过期/超量缓存

### 4.7 pkg/paramid/ — 参数注入中间件

- 从环境变量 `VALKEY_ADDR` 读取 Valkey 地址
- 请求到达时，从 query 中提取 `param_id`，从 Valkey 读取对应 JSON
- 将 JSON 中的键值对合并到请求的 query 参数中
- key 模板：`VALKEY_KEY_TEMPLATE`（默认 `amiabot-pages:params:{id}`）

## 5. API 路由说明

### 健康检查

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 返回 `{"status": "ok"}` |

### Bilibili 模块 (`/bilibili`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/bilibili/video` | 视频信息卡片页面。参数：`bvid` 或 `aid` |

### Pixiv 模块 (`/pixiv`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/pixiv/illust/info` | 插画信息页。参数：`id`（插画 ID） |
| GET | `/pixiv/illust/media` | 插画媒体信息 |
| GET | `/pixiv/image` | Pixiv 图片代理。参数：`url` |
| GET | `/pixiv/ugoira/gif` | ugoira 动图转 GIF 代理。参数：`id` |

### PJSK 模块 (`/pjsk`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/pjsk/event` | 活动页面。参数：`id` |
| GET | `/pjsk/event/current` | 当前活动跳转（302 重定向到当期活动页） |
| GET | `/pjsk/card` | 卡面页面。参数：`id` |
| GET | `/pjsk/music` | 音乐页面。参数：`id` |
| GET | `/pjsk/profile` | 玩家 Profile 页。参数：`uid` 等 |
| GET | `/pjsk/masterdata/*path` | MasterData JSON 代理 |
| GET | `/pjsk/assets/:label` | 资源二进制代理（图片等），带长期缓存头 |

### Status 模块 (`/status`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/status/zeabur` | Zeabur 服务器状态页面 |

所有路由组均可通过 `param_id` query 参数触发参数注入（需启用 Valkey）。

## 6. 构建、运行、测试、部署

### 本地运行

```bash
cd amiabot-pages
go mod download
go run .                    # 默认监听 :8080
PORT=3000 go run .          # 自定义端口
```

### 测试

```bash
go test ./...               # 运行所有测试
go test -v ./...            # 详细输出
go test ./handlers/pixiv/   # 只测试 pixiv 模块
go test ./handlers/pjsk/    # 只测试 pjsk 模块
go test ./pkg/paramid/      # 只测试 paramid 中间件
```

### 代码质量

```bash
gofmt -w .                  # 格式化代码
go vet ./...                # 静态检查
```

### Docker 构建与运行

```bash
# 使用 docker compose（推荐）
docker compose up -d --build

# 手动构建
docker build -t amiabot-pages .
docker run -d -p 8080:8080 amiabot-pages
```

### CI/CD

推送到 `main` 分支自动触发 GitHub Actions：
1. `build-and-check` — 下载依赖、编译、测试
2. `docker-build-push` — 构建 Docker 镜像并推送到 Docker Hub（`xiaocaoooo/amiabot-pages:latest` 和 `:sha-<commit>`）

## 7. 环境变量配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `PORT` | `8080` | HTTP 监听端口 |
| `SEKAI_ASSET` | `snowy,uni,haruki` | PJSK 图片源优先级（逗号分隔） |
| `PIXIV_ACCESS_TOKEN` | 空 | Pixiv API 访问令牌 |
| `PIXIV_REFRESH_TOKEN` | 空 | Pixiv token 刷新凭证 |
| `PIXIV_TAG_BLACKLIST` | 空 | Pixiv 标签黑名单过滤（逗号分隔） |
| `GITHUB_TOKEN` | 空 | GitHub API token（提升请求限额） |
| `ZEABUR_TOKEN` | 空 | Zeabur GraphQL API 认证令牌 |
| `IMAGE_CACHE_MAX_SIZE` | `512` | 图片缓存上限（MB） |
| `PJSK_PROFILE_BASEURL` | 空 | PJSK Profile 上游 API 地址 |
| `PJSK_PROFILE_HEADERS` | `{}` | 上游请求头（JSON 字符串） |
| `VALKEY_ADDR` | 空 | Valkey/Redis 地址（启用 param_id 中间件） |
| `VALKEY_KEY_TEMPLATE` | `amiabot-pages:params:{id}` | param_id 的 key 模板 |

## 8. 缓存策略说明

本项目使用多层缓存策略：

### PJSK MasterData JSON 缓存
- 从 GitHub API 获取最新 commit SHA
- 与本地缓存的 SHA 对比，仅在变化时重新下载
- 缓存文件存放在 `cache/` 目录

### 图片缓存（pkg/imgcache）
- 远程图片下载后计算 SHA256 哈希作为文件名
- 存储为 base64 dataURL 格式，可直接嵌入 HTML
- 支持 `IMAGE_CACHE_MAX_SIZE` 容量上限（默认 512MB）
- 每 10 分钟自动清理过期/超量缓存
- 启动时从磁盘加载已有缓存索引

### 静态资源缓存
- PJSK 资源二进制代理接口（`/pjsk/assets/:label`）设置 HTTP 头：
  `Cache-Control: public, max-age=31536000, immutable`

## 9. 开发约定

### 代码风格
- 使用 `gofmt -w .` 格式化代码
- 使用 `go vet ./...` 进行静态检查
- 提交前确保 `go build ./...` 和 `go test ./...` 通过

### 包组织
- handler 按功能域分包：`handlers/bilibili/`、`handlers/pixiv/`、`handlers/pjsk/`、`handlers/status/`
- 通用工具放 `pkg/`：`pkg/imgcache/`（图片缓存）、`pkg/paramid/`（参数注入中间件）
- 每个 handler 包独立管理自己的逻辑，不跨包引用其他 handler

### 模板约定
- 所有模板通过 `r.LoadHTMLFiles()` 显式加载（不使用 `LoadHTMLGlob`）
- 模板使用 `{{define}}` 块定义
- `layout.html` 定义全局布局和 CSS 变量（Material Design 3 风格）
- 新增页面需同时：① 创建模板文件 ② 在 `main.go` 的 `LoadHTMLFiles` 中注册

### 错误处理
- **所有 handler 出错时渲染带有 `Error` 字段的 HTML 卡片页面**，而非返回 JSON
- 这确保截图场景下用户能看到可视化错误反馈，而非空白或 JSON 文本

### Docker 安全
- 容器以非 root 用户 `appuser`（UID 10001）运行
- 多阶段构建，最终镜像基于 alpine:3.20，体积最小化

### 多源回退
- PJSK 图片资源支持三源（snowy/uni/haruki）顺序尝试
- 每个源最多 2 轮重试
- 源优先级由 `SEKAI_ASSET` 环境变量配置

## 10. 常见任务指引

### 如果你要添加一个新页面

1. 在 `handlers/<domain>/` 下创建 handler 文件（如 `handlers/pjsk/newpage.go`）
2. 在 `templates/<domain>/` 下创建模板文件（如 `templates/pjsk/newpage.html`）
3. 在 `main.go` 中：
   - 在 `r.LoadHTMLFiles()` 中添加新模板路径
   - 在对应的路由组中注册新路由（如 `pjskGroup.GET("/newpage", pjsk.NewPageHandler)`）
4. handler 函数签名：`func NewPageHandler(c *gin.Context)`
5. 渲染模板：`c.HTML(http.StatusOK, "newpage.html", gin.H{...})`
6. 错误处理：出错时渲染带 `Error` 字段的同一模板，而非返回 JSON

### 如果你要添加一个新的功能模块（新路由组）

1. 创建 `handlers/<newmodule>/` 目录，编写 handler
2. 创建 `templates/<newmodule>/` 目录，编写模板
3. 在 `main.go` 中：
   - import 新的 handler 包
   - 在 `r.LoadHTMLFiles()` 中注册新模板
   - 创建新路由组：`newGroup := r.Group("/newmodule", groupMiddlewares...)`
   - 注册路由
4. 如果需要缓存，在 `pkg/` 下创建对应工具包

### 如果你要修改模板样式

- 全局样式在 `templates/layout.html` 中定义（CSS 变量，Material Design 3 风格）
- 修改全局样式只需编辑 `layout.html`
- 页面特定样式在各自的模板文件 `<style>` 块中定义

### 如果你要修改缓存策略

- 图片缓存逻辑在 `pkg/imgcache/imgcache.go`
- PJSK JSON 增量更新逻辑在 `handlers/pjsk/assets.go`
- 资源二进制缓存头在 `handlers/pjsk/asset_binary.go`

### 如果你要调试 Pixiv 相关功能

- 设置环境变量 `PIXIV_ACCESS_TOKEN` 和 `PIXIV_REFRESH_TOKEN`
- 测试运行：`go test -v ./handlers/pixiv/`

### 如果你要调试 PJSK 相关功能

- MasterData 会自动从 GitHub 下载（可能需要 `GITHUB_TOKEN` 提升限额）
- 图片源由 `SEKAI_ASSET` 控制，默认 `snowy,uni,haruki`
- 测试运行：`go test -v ./handlers/pjsk/`

### 如果你要修改 param_id 中间件

- 中间件逻辑在 `pkg/paramid/middleware.go`
- 测试使用 miniredis mock，不需要真实 Redis：`go test -v ./pkg/paramid/`

## 11. 项目间关系

```
┌─────────────────┐      HTTP       ┌──────────────────┐
│  AmiaBot 插件   │ ──────────────→ │  amiabot-pages   │
│  (Python)       │   拼接页面URL   │  (Go + Gin)      │
│                 │ ←────────────── │                  │
│  amiabot_pages  │   返回 HTML     │  渲染卡片页面    │
│  配置项指定URL  │                 │                  │
└────────┬────────┘                 └──────────────────┘
         │
         │ 调用截图插件
         ▼
┌─────────────────┐
│  截图插件       │
│  (Puppeteer等)  │
│  HTML → 图片    │
└─────────────────┘
```

- **上游调用方**：AmiaBot 外置插件（Python），通过 `amiabot_pages` 配置项指定本服务的 URL 前缀
- **调用流程**：插件拼接页面 URL（含参数）→ HTTP GET 请求本服务 → 返回渲染好的 HTML → 插件调用截图工具截取页面 → 发送图片给用户
- **独立部署**：本服务独立运行，通过 HTTP 对外提供服务，不依赖 AmiaBot 主进程
