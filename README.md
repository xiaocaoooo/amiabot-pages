# amiabot-pages

基于 Go + Gin 的页面服务，用于渲染 AmiaBot 相关信息卡片页面。

当前包含两类页面能力：

- `Bilibili` 视频信息卡片（封面、UP 主、统计、分 P）
- `PJSK` 活动页与卡面页（多服数据、活动进度、卡面预览）

## 特性

- 统一的 HTML 模板与视觉风格，适合生成截图或卡片展示
- 启动后自动后台拉取 PJSK 多服务器 JSON 资源并缓存到本地
- PJSK 图片资源支持 `snowy/uni/haruki` 多源回退与失败重试
- 图片下载后转为 `data URL` 并缓存，减少重复外链请求
- 提供健康检查接口，方便容器编排与监控

## 技术栈

- Go `1.25.x`
- [gin-gonic/gin](https://github.com/gin-gonic/gin)

## 目录结构

```text
.
├── main.go                    # 服务入口与路由注册
├── handlers/
│   ├── bilibili/video.go      # B 站视频卡片
│   └── pjsk/
│       ├── assets.go          # PJSK JSON 资源下载与缓存
│       ├── asset_source.go    # PJSK 图片资源多源解析与重试
│       ├── event.go           # 活动页面 / 当前活动跳转
│       └── card.go            # 卡面页面
├── templates/                 # HTML 模板
├── static/pjsk/card/          # 卡框、属性图标、星星素材
└── cache/
    ├── pjsk/                  # PJSK JSON 缓存
    └── images/                # 图片 data URL 缓存
```

## 快速开始

### 1) 本地运行

```bash
go mod download
go run .
```

默认监听 `8080` 端口。

### 2) Docker Compose

```bash
docker compose up -d --build
```

默认映射 `8080:8080`。

## 环境变量

| 变量名 | 默认值 | 说明 |
|---|---|---|
| `PORT` | `8080` | HTTP 服务端口 |
| `GITHUB_TOKEN` | 空 | 可选。用于调用 GitHub API 拉取 PJSK 仓库信息时提升限额 |
| `IMAGE_CACHE_MAX_SIZE` | `512` | 图片缓存上限（单位 MB） |
| `SEKAI_ASSET` | `snowy,uni,haruki` | PJSK 图片资源源优先级。支持 `snowy` / `uni` / `haruki`，可用逗号配置多个，按顺序回退重试 |

## 接口说明

### 健康检查

- `GET /health`
- 响应：`{"status":"ok"}`

### Bilibili

- `GET /bilibili/video`
- 参数：
  - `bv` / `bvid`：BV 号
  - `av` / `aid` / `avid`：AV 号

示例：

```text
/bilibili/video?bv=BV1XY411A7c2
/bilibili/video?av=170001
```

说明：

- 若不传参数，会返回默认示例卡片页面
- 页面为 HTML（不是 JSON）

### PJSK 活动

- `GET /pjsk/event`
- 参数：
  - `id`：活动 ID（可选）
  - `server`：服务器，支持 `jp/cn/en/tw/kr`（默认 `jp`）

示例：

```text
/pjsk/event?id=100&server=jp
/pjsk/event?server=cn
```

说明：

- 当未传 `id` 时，会重定向到 `/pjsk/event/current?server=...`

### PJSK 当前活动

- `GET /pjsk/event/current`
- 参数：
  - `server`：服务器，支持 `jp/cn/en/tw/kr`（默认 `jp`）

说明：

- 从本地缓存的 `events.json` 中按活动时间（`startAt/aggregateAt/closedAt`）选择“最新活动”（不要求当前进行中）
- 成功后 302 跳转到 `/pjsk/event?id=xxx&server=...`

### PJSK 卡面

- `GET /pjsk/card`
- 参数：
  - `id`：卡面 ID（必填）
  - `server`：服务器，支持 `jp/cn/en/tw/kr`（默认 `jp`）

示例：

```text
/pjsk/card?id=1001&server=jp
```

### PJSK 资源缓存接口

- `GET /pjsk/assets/refresh`
  - 触发全服务器缓存刷新
  - 可选参数：`force=true`（忽略 commit SHA，强制下载）
- `GET /pjsk/assets/{repo}/{file}`
  - 读取本地缓存 JSON

示例：

```text
/pjsk/assets/refresh
/pjsk/assets/refresh?force=true
/pjsk/assets/sekai-master-db-diff/events.json
/pjsk/assets/sekai-master-db-cn-diff/events.json
```

## 缓存机制说明

### PJSK JSON 缓存

- 缓存目录：`cache/pjsk/`
- 启动后会后台执行一次全量刷新（按服务器并发）
- 每个服务器会记录最新 commit SHA 到 `.commit_sha`
- 若远端 commit 未变化且本地文件完整，则跳过下载

### 图片缓存

- 缓存目录：`cache/images/`
- 以 URL 的 SHA256 作为文件名，缓存内容为 `data URL`
- 启动时加载索引，并每 10 分钟清理过期/超限缓存
- `ttl < 0` 的资源视为不过期（当前业务多数使用此策略）

## 注意事项

- 服务启动后，PJSK 资源是后台拉取；若首次访问过早，可能出现“缓存文件不存在”提示，可稍后重试或手动调用 `/pjsk/assets/refresh`
- 运行环境需能访问以下外部站点：
  - `api.github.com`
  - `sekai-world.github.io`
  - `snowyassets.exmeaning.com`
  - `assets.unipjsk.com`
  - `sekai-assets-bdf29c81.seiunx.net`
  - `api.bilibili.com`

## 开发建议

```bash
# 格式化
gofmt -w .

# 静态检查（如果本地安装了 go vet 相关工具链）
go vet ./...

# 编译
go build .
```
