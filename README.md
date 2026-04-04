# amiabot-pages

基于 Go + Gin 的页面服务，用于渲染 AmiaBot 相关信息卡片页面。

当前包含三类页面能力：

- `Bilibili` 视频信息卡片（封面、UP 主、统计、分 P）
- `Pixiv` 插画信息页（主图、作者、标签、统计、多页预览）
- `PJSK` 活动页、卡面页、音乐页与玩家 Profile 页（多服数据、活动进度、卡面预览、玩家卡组与游玩统计）

## 特性

- 统一的 HTML 模板与视觉风格，适合生成截图或卡片展示
- 启动后自动后台拉取 PJSK 多服务器 JSON 资源并缓存到本地
- PJSK 图片资源支持 `snowy/uni/haruki` 多源回退与失败重试
- 图片下载后转为 `data URL` 并缓存，减少重复外链请求
- 支持通过 `param_id` 从 Valkey 注入预存 query 参数，解决 URL 过长问题
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
│   ├── pixiv/
│   │   ├── client.go          # Pixiv App API / OAuth 请求封装
│   │   └── illust.go          # Pixiv 插画信息页
│   └── pjsk/
│       ├── assets.go          # PJSK JSON 资源下载与缓存
│       ├── asset_source.go    # PJSK 图片资源多源解析与重试
│       ├── event.go           # 活动页面 / 当前活动跳转
│       ├── card.go            # 卡面页面
│       └── profile.go         # 玩家 Profile 页面
├── templates/                 # HTML 模板
├── pkg/paramid/               # param_id -> Valkey 参数注入中间件
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
| `PIXIV_ACCESS_TOKEN` | 空 | 可选。Pixiv App API access token；也可在请求头 `Authorization: Bearer <token>` 传入 |
| `PIXIV_REFRESH_TOKEN` | 空 | 可选但推荐。用于自动换取/刷新 Pixiv access token，供 `/pixiv/illust/info` 使用 |
| `PIXIV_TAG_BLACKLIST` | 空 | 可选。Pixiv 标签黑名单；命中后不在页面展示。支持按标签原名或翻译名匹配，大小写不敏感，支持用 `,` / `，` / `;` / `；` / 换行分隔 |
| `ZEABUR_TOKEN` | 空 | 可选。用于 `/status/zeabur` 页面查询 Zeabur GraphQL 状态（也可在请求头 `Authorization: Bearer <token>` 传入） |
| `IMAGE_CACHE_MAX_SIZE` | `512` | 图片缓存上限（单位 MB） |
| `SEKAI_ASSET` | `snowy,uni,haruki` | PJSK 图片资源源优先级。支持 `snowy` / `uni` / `haruki`，可用逗号配置多个，按顺序回退重试 |
| `PJSK_PROFILE_BASEURL` | 空 | 可选。PJSK Profile 上游 API 基础地址，例如 `https://example.com/api`；`/pjsk/profile` 会请求 `{baseurl}/{server}/{id}/profile` |
| `PJSK_PROFILE_HEADERS` | 空 / `{}` | 可选。PJSK Profile 上游请求头，格式为 JSON 对象字符串，例如 `{"x-moe-sekai-token":"<token>"}` 或 `{"Authorization":"Bearer <token>"}` |
| `PJSK_SUITE_BASEURL` | 空 | 可选。PJSK Suite API 基础地址，例如 `https://suite-api.haruki.seiunx.com`；`/pjsk/b30` 会请求 `{baseurl}/public/{server}/suite/{id}` 获取玩家打歌成绩 |
| `VALKEY_ADDR` | 空 | Valkey 地址（示例：`127.0.0.1:6379`）。为空时不启用 `param_id` 注入功能 |
| `VALKEY_PASSWORD` | 空 | 可选。Valkey 密码 |
| `VALKEY_DB` | `0` | 可选。Valkey DB 编号 |
| `VALKEY_KEY_TEMPLATE` | `amiabot-pages:params:{id}` | 可选。`param_id` 到 key 的模板，必须包含 `{id}` |

## `param_id` 注入用法（通用）

当目标端点只能使用 URL 参数、但参数过长时，可通过 `param_id` 先在外部系统存储参数，再由本服务读取注入。

适用范围：

- `GET /status/*`
- `GET /bilibili/*`
- `GET /pixiv/*`
- `GET /pjsk/*`
- `GET /health` 不受影响

流程：

1. 外部系统将大参数写入 Valkey（建议设置 TTL），并得到参数 ID（如 `abc123`）
2. 调用目标端点时只传 `param_id=abc123`
3. 服务读取 key 并将参数合并到请求 query 后继续执行原 handler

key 生成规则：

- 默认模板：`amiabot-pages:params:{id}`
- 示例：`param_id=abc123` 对应 key 为 `amiabot-pages:params:abc123`
- 可通过 `VALKEY_KEY_TEMPLATE` 覆盖模板

Valkey value 格式（JSON 对象）：

```json
{
  "server": "jp",
  "id": "1001",
  "tag": ["a", "b"],
  "retry": 3,
  "preview": true
}
```

规则：

- 支持标量值：`string / number / bool`
- 支持数组值，转为重复 query 参数（如 `tag=a&tag=b`）
- 不支持嵌套对象，遇到将返回 `400`
- URL 显式参数优先于 Valkey 参数（可用于临时覆盖调试）
- `param_id` 在注入后会被移除，不传递给业务 handler

示例：

```text
/pjsk/card?param_id=abc123
/pjsk/card?param_id=abc123&id=2002   # URL 中 id 会覆盖 Valkey 中 id
/bilibili/video?param_id=video_job_9
/pixiv/illust/info?param_id=pixiv_job_1
```

错误语义：

- `400 Bad Request`
  - `param_id` 为空
  - `param_id` 未命中 / 已过期
  - Valkey value 不是合法 JSON 对象或包含不支持类型
- `502 Bad Gateway`
  - Valkey 不可用或读取失败

错误响应示例：

```json
{"error":"param_id 无效或已过期"}
```

```json
{"error":"Valkey 不可用: dial tcp 127.0.0.1:6379: connect: connection refused"}
```

## 接口说明

### 健康检查

- `GET /health`
- 响应：`{"status":"ok"}`

### Zeabur 状态

- `GET /status/zeabur`
- 认证：
  - 优先读取请求头：`Authorization: Bearer <token>`
  - 未提供请求头时，回退使用环境变量 `ZEABUR_TOKEN`
- 响应：
  - 返回 HTML 页面，按现有卡片风格展示服务器/项目/服务状态信息

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

### Pixiv 插画信息

- `GET /pixiv/illust/info`
- 参数：
  - `pid`：插画 PID（必填）
- 鉴权优先级：
  - 请求头 `Authorization: Bearer <access_token>`
  - 环境变量 `PIXIV_ACCESS_TOKEN`
  - 环境变量 `PIXIV_REFRESH_TOKEN`（自动换取并缓存 access token）

示例：

```text
/pixiv/illust/info?pid=125547965
/pixiv/illust/info?param_id=abc123
```

说明：

- 页面为 HTML（不是 JSON）
- 若 access token 失效且配置了 `PIXIV_REFRESH_TOKEN`，服务会自动刷新并重试一次
- 可通过环境变量 `PIXIV_TAG_BLACKLIST` 过滤不希望展示的标签
  - 同时匹配标签原名与翻译名
  - 匹配大小写不敏感
  - 示例：`PIXIV_TAG_BLACKLIST="R-18,原创,nsfw"`
- 页面会展示主图、作者、标签、统计信息、系列信息与多页预览

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

### PJSK 玩家 Profile

- `GET /pjsk/profile`
- 参数：
  - `id`：玩家 ID（必填）
  - `server`：服务器，支持 `jp/cn/en/tw/kr`（默认 `jp`）

示例：

```text
/pjsk/profile?id=1234567890123456&server=jp
```

说明：

- 该页面会请求外部 PJSK Profile API，并渲染玩家基础资料、主力卡组与游玩统计
- 需要通过环境变量配置上游地址：`PJSK_PROFILE_BASEURL`
- 如上游需要鉴权，可通过 `PJSK_PROFILE_HEADERS` 传入任意请求头
- 例如对接 Moe-Sekai API：

```text
PJSK_PROFILE_BASEURL=https://example.com/api
PJSK_PROFILE_HEADERS={"x-moe-sekai-token":"<token>"}
```

### PJSK B30

- `GET /pjsk/b30`
- 参数：
  - `id`：玩家 ID（必填）
  - `server`：服务器，支持 `jp/cn/en/tw/kr`（默认 `jp`）

示例：

```text
/pjsk/b30?id=7486859250443000614&server=cn
```

说明：

- 该页面展示玩家 Top 30 打歌成绩（按 rating 排序）
- 需要通过 `PJSK_SUITE_BASEURL` 配置 Suite API 地址以获取打歌成绩
- 可选通过 `PJSK_PROFILE_BASEURL` + `PJSK_PROFILE_HEADERS` 获取玩家昵称
- 例如对接 Haruki Suite API + Moe-Sekai Profile API：

```text
PJSK_SUITE_BASEURL=https://suite-api.haruki.seiunx.com
PJSK_PROFILE_BASEURL=https://seka-api.exmeaning.com/api
PJSK_PROFILE_HEADERS={"x-moe-sekai-token":"<token>"}
```

### PJSK MasterData 缓存接口

- `GET /pjsk/masterdata/refresh`
  - 触发全服务器缓存刷新
  - 可选参数：`force=true`（忽略 commit SHA，强制下载）
- `GET /pjsk/masterdata/{repo}/{file}`
  - 读取本地缓存 JSON

示例：

```text
/pjsk/masterdata/refresh
/pjsk/masterdata/refresh?force=true
/pjsk/masterdata/sekai-master-db-diff/events.json
/pjsk/masterdata/sekai-master-db-cn-diff/events.json
```

### PJSK 资源二进制接口

- `GET /pjsk/assets/{label}`
  - 直接返回资源二进制内容
  - 可选参数：`server=jp/cn/en/tw/kr`（默认 `jp`）
  - 返回带 `Cache-Control: public, max-age=31536000, immutable`，并复用服务内图片缓存

示例：

```text
/pjsk/assets/event:background:event_marathon_001?server=jp
/pjsk/assets/card:thumbnail:res001_no001_rip:normal?server=jp
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

- 服务启动后，PJSK MasterData 是后台拉取；若首次访问过早，可能出现“缓存文件不存在”提示，可稍后重试或手动调用 `/pjsk/masterdata/refresh`
- 运行环境需能访问以下外部站点：
  - `api.github.com`
  - `sekai-world.github.io`
  - `sekai-api.exmeaning.com`
  - `snowyassets.exmeaning.com`
  - `assets.unipjsk.com`
  - `sekai-assets-bdf29c81.seiunx.net`
  - `api.bilibili.com`
  - `app-api.pixiv.net`
  - `oauth.secure.pixiv.net`
  - `i.pximg.net`

## 开发建议

```bash
# 格式化
gofmt -w .

# 静态检查（如果本地安装了 go vet 相关工具链）
go vet ./...

# 编译
go build .
```
