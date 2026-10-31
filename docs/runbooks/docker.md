# Docker 镜像打包、发布与部署

本文命令从仓库根目录执行。包含源码构建、镜像仓库发布、离线交付、生产配置、首次管理员、备份及更新恢复。
涉及交互输入的示例使用 Bash；zsh 用户先执行 `bash`。示例域名、仓库地址和版本号均需替换。

## 1. 交付结构与前置条件

| 组件 | 镜像/容器 | 作用 |
| --- | --- | --- |
| Rust API | `wekeeper-api` / `api` | Axum 业务接口，容器内 8080，启动时执行 SQLx migration |
| React + Caddy | `wekeeper-web` / `web` | 静态页面、同源 API 代理，容器内 80/443 |
| PostgreSQL | `postgres:17.4-bookworm` / `postgres` | 业务、会话、媒体及审计数据 |

`api-contract` 参与前端构建，不单独部署。旧 PHP、uni-app、Java、千寻与历史资源不进入新镜像。
生产使用独立 PostgreSQL；嵌入式 PostgreSQL 仅用于源码开发。

准备支持 BuildKit 的 Docker Engine、支持 `up --wait` 的 Compose v2，以及足够的磁盘与构建内存。
构建机需访问基础镜像仓库、GitHub、crates.io、npm 和 Debian 包源；仅运行镜像的服务器无需安装 Rust、Node、pnpm。

```bash
docker version
docker compose version
docker buildx version
```

关键文件：

- `backend/Dockerfile`：Rust 1.97.1 多阶段构建，最终 Debian slim 镜像以 UID/GID 10001:10001 运行。构建和运行阶段的 APT 使用中科大镜像；Cargo 使用默认的 crates.io 公共仓库。基础镜像仍使用官方 Rust/Debian 镜像，与参考项目一致。
- `frontend/Dockerfile`：Node.js 22 构建 contract/React，Caddy 2.10 提供运行环境。
- `.dockerignore`：仅纳入新工程目录，排除 target、node_modules、dist、环境文件及历史目录。
- `compose.yaml`：生产运行基线，只引用镜像，没有 build 配置。
- `compose.dev.yaml`：补充源码构建及 PostgreSQL/API 回环调试端口。
- `env.example`：运行配置模板。

**两个 Dockerfile 的构建上下文均为仓库根目录 `.`**，不能在子目录以该子目录为上下文构建。

## 2. 构建前检查

已有源码工具链时执行；也可全部通过 Devbox 运行：

```bash
devbox run -- make install
pnpm --dir api-contract build
devbox run -- make check-web check-backend
devbox run -- make test-business
```

没有全局 pnpm 时，将第二条改成 `devbox run -- pnpm --dir api-contract build`。
`api-contract build` 是类型检查；前端 Dockerfile 同样安装并检查共享契约。
`check-backend` 包含普通 cargo build，避免仅测试构建掩盖生产依赖 feature 缺失。
`test-business` 使用临时隔离数据库，不使用生产连接。

## 3. 本地打包与完整容器栈

首次准备配置；已有 `.env` 时直接编辑，避免覆盖：

```bash
cp env.example .env
chmod 600 .env
```

本机 HTTP 调试至少修改：

```dotenv
POSTGRES_PASSWORD=替换为长随机密码
API_IMAGE=wekeeper-api:local
WEB_IMAGE=wekeeper-web:local
WEKEEPER_PUBLIC_ORIGIN=http://127.0.0.1:8088
WEKEEPER_INSECURE_LOCAL_AUTH=true
```

先验证配置，再构建并等待健康：

```bash
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml config --quiet
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml build --pull
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up -d --wait --wait-timeout 180
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml ps
```

也可用 `make compose-up` 合并构建和启动；`make compose-config` 检查开发栈配置。
完整 `docker compose config` 会包含插值后的密码，日常语法检查使用 `--quiet`。

入口：

- 后台：`http://127.0.0.1:8088/admin`
- 社区：`http://127.0.0.1:8088/community`
- 企业微信预览：`http://127.0.0.1:8088/wecom`
- API 存活/就绪：`/healthz`、`/readyz`

Docker 首次启动**不会自动创建管理员**，按第 7 节初始化。
HTTP 普通浏览器可预览草稿；企业微信 OAuth/SDK 发送需要真实 HTTPS 入口与企业权限。

```bash
make smoke
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml logs --tail 100 api web
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml down
```

`down` 保留数据卷。已有数据的环境不要使用 `down --volumes`。
开发与生产应使用不同目录/环境文件，并用不同 `-p` 项目名隔离卷，例如 `-p wekeeper-dev`。
一旦选择项目名，后续所有操作都使用相同项目名。

## 4. 构建并发布版本镜像

准备镜像仓库及一次发布的唯一版本号：

```bash
export WK_REGISTRY=registry.example.com/team
export WK_VERSION=2026.10.21-abcdef0
export API_IMAGE="$WK_REGISTRY/wekeeper-api:$WK_VERSION"
export WEB_IMAGE="$WK_REGISTRY/wekeeper-web:$WK_VERSION"
docker login registry.example.com

docker build --pull -f backend/Dockerfile -t "$API_IMAGE" .
docker build --pull -f frontend/Dockerfile -t "$WEB_IMAGE" .
docker push "$API_IMAGE"
docker push "$WEB_IMAGE"
```

版本号应对应实际 commit，不重复覆盖发布标签。记录源码 commit、构建时间、API/Web digest、扫描结果和 migration 版本。

```bash
docker image inspect "$API_IMAGE" --format '{{json .RepoDigests}}'
docker image inspect "$WEB_IMAGE" --format '{{json .RepoDigests}}'
```

生产 `.env` 优先填 `registry.example.com/team/wekeeper-api@sha256:实际摘要` 等 digest 引用。
### 构建时 GitHub 依赖下载失败

后端的 `cairnkit-jsonapi` 是固定提交的 GitHub 依赖。Cargo 默认使用 libgit2 拉取，和命令行 `git` 的实现、配置读取并不完全相同，因此系统 Git 能签出时，Cargo 仍可能出现 `class=Os (2)`、连接超时或 SSL 错误；仅凭这些日志不能认定网络不可达。

Dockerfile 安装系统 Git 和 CA 证书，并设置 `CARGO_NET_GIT_FETCH_WITH_CLI=true`，让 Cargo 使用系统 Git。先执行 `cargo fetch --locked --verbose`，再执行 `cargo build --frozen --release`：下载阶段保留详细日志，编译阶段使用已下载且锁定的依赖，不再联网。Git 与 registry 缓存均由 BuildKit 持久化。不配置代理、私有仓库或关闭证书验证。

使用更新后的文件重新构建，检查详细下载日志：

```bash
docker build --pull --progress=plain -f backend/Dockerfile -t "$API_IMAGE" .
```

若系统 Git 拉取仍失败，检查日志中的 Git 错误及构建容器的 DNS、CA 证书和网络环境。宿主机的 Git 配置不会自动进入构建容器，宿主机签出成功不等于构建容器签出成功。

`--pull` 更新基础镜像缓存，不保证可重现的基础镜像 digest；有严格重现要求时需另行固定 Dockerfile 基础镜像摘要。

API 构建通过 BuildKit cache mount 缓存 Cargo registry、Git 和 target；同一 builder 可复用。
不要为了普通增量发布常规使用 `--no-cache`。临时 CI 的跨机器缓存需要另外配置，不是当前命令自动提供的。

上述普通构建面向构建机原生平台。服务器架构不同时，可使用 Buildx：

```bash
docker buildx create --name wekeeper-release --use
docker buildx inspect --bootstrap
docker buildx build --platform linux/amd64 --pull -f backend/Dockerfile -t "$API_IMAGE" --push .
docker buildx build --platform linux/amd64 --pull -f frontend/Dockerfile -t "$WEB_IMAGE" --push .
```

已有同名 builder 时使用 `docker buildx use wekeeper-release`。
ARM 服务器改为 `linux/arm64`，前提是 builder 支持该目标及所需仿真。
跨架构产物需要在目标主机验收；本文不把命令示例等同于多架构已验证。

### 离线镜像交付

在联网构建机保存两个应用镜像和数据库镜像：

```bash
docker pull postgres:17.4-bookworm
docker save -o wekeeper-images.tar "$API_IMAGE" "$WEB_IMAGE" postgres:17.4-bookworm
sha256sum wekeeper-images.tar > wekeeper-images.tar.sha256
```

传输 tar、校验文件、`compose.yaml`、`env.example` 到目标主机，配置目标 `.env`：

```bash
sha256sum -c wekeeper-images.tar.sha256
docker load -i wekeeper-images.tar
docker compose --env-file .env -f compose.yaml config --quiet
docker compose --env-file .env -f compose.yaml up --no-build --pull never -d --wait --wait-timeout 180
```

离线模式 `.env` 使用归档中实际加载的标签；不要假设 `docker save/load` 后仍具备仓库 digest 元数据。
不执行 pull。镜像离线交付不代表企业微信可离线运行，API 与客户端仍需访问腾讯官方服务。

## 5. 分别配置前端、后端与数据库

先选择启动方式：使用仓库 `compose.yaml` 时，编辑一份 `.env`，由 Compose 分别给三个容器注入变量；使用 `docker run` / 1Panel 独立容器时，分别填写下文的 `.env.api`、`.env.web` 和 `.env.postgres`。两种方式任选一种，不要同时启动占用相同端口的两套容器。

### 生产账号和密码从哪里获取

生产部署没有可直接登录的默认管理员密码。数据库连接账号与 `/admin` 后台登录账号是两套独立账号：

| 账号 | 用户名来源 | 密码来源 | 用途 |
| --- | --- | --- | --- |
| 仓库 Compose 新建的 PostgreSQL | `POSTGRES_USER`，模板为 `wekeeper` | 部署者首次启动前设置的 `POSTGRES_PASSWORD`，没有默认密码 | API 连接数据库，不用于后台登录 |
| 已有生产 PostgreSQL | 数据库管理员或云数据库控制台提供的应用角色 | 该角色实际配置的密码 | 填入后端 `WEKEEPER_DATABASE_USER` / `WEKEEPER_DATABASE_PASSWORD` |
| WeKeeper 后台管理员 | 第 7 节创建命令指定，示例为 `admin` | 执行创建命令时自行输入，12–128 字节 | 登录 `/admin` |

**新建数据库：** 在首次启动前用 `openssl rand -hex 32` 生成密码，保存到 Compose `.env` 的 `POSTGRES_PASSWORD`。独立容器则将同一密码分别填入 `.env.postgres` 的 `POSTGRES_PASSWORD` 和 `.env.api` 的 `WEKEEPER_DATABASE_PASSWORD`。模板中的 `replace-with-a-long-random-password` 只是待替换文本，不是项目提供的生产密码。

**使用已有生产数据库：** 向维护者获取数据库主机、端口、库名、应用用户名、密码及 TLS 要求，填入第 5.3 节对应变量。程序默认用户名 `wekeeper` 仅是默认连接值，不会在外部数据库自动创建这个角色。API 启动会执行 migration，所用角色必须具备该业务库/模式内创建和修改表、索引等权限；由数据库管理员为 WeKeeper 准备专用库及授权。

**已经部署但忘记数据库密码：** 先查部署时保存的 `.env`、`.env.api`、平台环境配置或密码管理系统。PostgreSQL 不提供原密码明文查询；找不到时由数据库管理员重设角色密码，再更新 API 配置并重建容器。仅修改 `POSTGRES_PASSWORD` 不会更改已有数据卷中的密码，也不要删除生产数据卷来重置账号。

**数据库连通后仍无法登录后台：** 新生产库没有自动生成的后台账号，必须执行 [第 7 节](#_7-首次管理员初始化)。数据库用户名/密码不能登录 `/admin`，嵌入式开发环境的临时管理员密码也不能用于生产。

### 5.1 构建变量与运行变量的区别

| 配置阶段 | 配置位置 | 内容 |
| --- | --- | --- |
| 制作镜像 | 构建终端 `export` | `WK_REGISTRY`、`WK_VERSION` 用来组成 `API_IMAGE`、`WEB_IMAGE` 标签；不是应用配置 |
| 前端镜像构建 | `frontend/Dockerfile` | 当前没有业务环境变量或 API 地址构建参数；同一镜像可部署到不同环境 |
| 后端镜像构建 | `backend/Dockerfile` | 不传数据库、CorpID、Secret 或域名；这些值均在启动时注入 |
| 前端容器运行 | Web 容器 environment | Caddy 的 `WEKEEPER_WEB_LISTEN`、`WEKEEPER_API_UPSTREAM` |
| 后端容器运行 | API 容器 environment | 数据库、浏览器 Origin、登录模式、企微凭据、同步间隔及日志 |
| Compose 编排 | `.env` | 镜像名、宿主机端口、数据库初始化值，以及 Compose 明确引用的应用变量 |

React 使用同源 `/api/...` 请求，由 Web 容器的 Caddy 转发到 API。不要给 React 填内部地址 `http://api:8080`，浏览器无法解析这个容器名。`WEKEEPER_API_ORIGIN` 仅用于源码开发的 Vite 代理，不是生产 Web 容器的配置；生产应使用 `WEKEEPER_API_UPSTREAM`。

`docker build` 不会自动读取 `.env`。`docker run` 必须显式使用 `--env-file` 或 `-e`。后端程序和前端静态文件本身也不会读取宿主机 `.env`。

### 5.2 前端 Web 容器变量

| 变量 | 示例 | 作用 |
| --- | --- | --- |
| `WEKEEPER_WEB_LISTEN` | `:80` | Caddy 监听 HTTP；直接提供 HTTPS 时改为 `keeper.example.com`，详见第 6 节 |
| `WEKEEPER_API_UPSTREAM` | `api:8080` | Caddy 可访问的后端地址；同一 Docker 网络内使用 API 容器名和容器端口 |

前端不需要数据库密码、CorpSecret 或 AgentID。`WEKEEPER_PUBLIC_ORIGIN` 是后端验证浏览器来源的配置，不是前端监听变量。

`WEB_BIND_HOST`、`WEB_HTTP_PORT`、`WEB_HTTPS_PORT` 是 Compose 的宿主机端口映射参数，不是 Caddy 环境变量。独立 `docker run` 使用 `-p` 设置映射。

### 5.3 后端 API 容器变量

| 变量 | 示例 / 默认值 | 作用 |
| --- | --- | --- |
| `WEKEEPER_BIND_ADDR` | `0.0.0.0:8080` | 容器内监听地址；不要使用 `127.0.0.1:8080`，否则 Web 容器无法访问 |
| `WEKEEPER_DATABASE_HOST` | `postgres` | 数据库容器名；外部数据库填写其主机名/IP |
| `WEKEEPER_DATABASE_PORT` | `5432` | 数据库服务端口，不是宿主机映射端口 |
| `WEKEEPER_DATABASE_NAME` | `wekeeper` | 数据库名 |
| `WEKEEPER_DATABASE_USER` | `wekeeper` | 数据库角色 |
| `WEKEEPER_DATABASE_PASSWORD` | 必填实际密码 | 分项密码，无需 URL 编码 |
| `WEKEEPER_DATABASE_SSL_MODE` | 同机容器示例 `disable`；程序默认 `prefer` | 远程生产数据库按证书条件使用 `verify-full` 并配置受信任 CA |
| `WEKEEPER_DATABASE_MAX_CONNECTIONS` | `10` | 连接池上限，允许 1–100 |
| `WEKEEPER_DATABASE_URL` | 可选，默认不设置 | 完整连接 URL；设置后优先于分项连接字段。不使用时删除该变量，不要填空值 |
| `WEKEEPER_PUBLIC_ORIGIN` | `https://keeper.example.com` | 浏览器实际访问的协议、域名、端口，不带路径和末尾斜杠；不能写 `api:8080` |
| `WEKEEPER_INSECURE_LOCAL_AUTH` | `false` | 本机 HTTP 调试设 `true`，生产 HTTPS 保持 `false` |
| `WEKEEPER_WECOM_CORP_ID` | 空或实际 CorpID | 企业微信服务端配置，与 Secret 成对填写 |
| `WEKEEPER_WECOM_SECRET` | 空或实际 Secret | 目录、客户群、成员核验及 OAuth 所需的应用凭据 |
| `WEKEEPER_WECOM_AGENT_ID` | 空或同一应用的正整数 AgentID | 当前客户端分享必需；目录同步与 OAuth 可留空 |
| `WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS` | `1800` | 内部通讯录周期同步；`0` 仅手动，非零至少 60 秒 |
| `WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS` | `1800` | 客户群周期同步；`0` 仅手动，非零至少 60 秒 |
| `WEKEEPER_WX_APP_ID` / `WEKEEPER_WX_SECRET` | 空 | 使用小程序能力时填写 |
| `RUST_LOG` | `wekeeper_api=info,tower_http=info` | 后端日志级别 |

### 5.4 使用仓库 Compose：一份 .env，分别注入

从 `env.example` 复制 `.env` 后，按用途分组填写。以下为本机 HTTP 完整示例；生产用第 6 节的 HTTPS 值替换前端入口和后端 Origin：

```dotenv
# 镜像与数据库容器
API_IMAGE=wekeeper-api:local
WEB_IMAGE=wekeeper-web:local
POSTGRES_IMAGE=postgres:17.4-bookworm
POSTGRES_DB=wekeeper
POSTGRES_USER=wekeeper
POSTGRES_PASSWORD=replace-with-a-long-random-password

# 前端 Web：宿主机端口 + Caddy 监听
WEB_BIND_HOST=127.0.0.1
WEB_HTTP_PORT=8088
WEB_HTTPS_PORT=8443
WEKEEPER_WEB_LISTEN=:80

# 后端 API：浏览器入口、登录、连接池、日志
WEKEEPER_PUBLIC_ORIGIN=http://127.0.0.1:8088
WEKEEPER_INSECURE_LOCAL_AUTH=true
WEKEEPER_DATABASE_MAX_CONNECTIONS=10
RUST_LOG=wekeeper_api=info,tower_http=info

# 后端 API：企业微信（需要时填写）
WEKEEPER_WECOM_CORP_ID=
WEKEEPER_WECOM_SECRET=
WEKEEPER_WECOM_AGENT_ID=
WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS=1800
WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS=1800
WEKEEPER_WX_APP_ID=
WEKEEPER_WX_SECRET=
```

替换密码及镜像名后，使用已制作好的镜像启动：

```bash
chmod 600 .env
docker compose --env-file .env -f compose.yaml config --quiet
docker compose --env-file .env -f compose.yaml up --no-build -d --wait --wait-timeout 180
```

Compose 已固定注入 API 监听 `0.0.0.0:8080`、数据库地址 `postgres:5432`、SSL `disable` 和 Web 上游 `api:8080`，并将 `POSTGRES_*` 转换为 API 的数据库分项配置。因此，单独在 `.env` 新增 `WEKEEPER_DATABASE_HOST`、`WEKEEPER_DATABASE_URL` 或 `WEKEEPER_API_UPSTREAM` **不会覆盖这些值或自动传入容器**。使用外部数据库/自定义上游时采用下一节独立配置，或显式修改编排中的 environment 和 depends_on。

Shell 已 export 的同名变量优先于 Compose `.env`；部署前核对 `API_IMAGE`、`WEB_IMAGE` 等，避免沿用构建终端的旧标签。密码可用 `openssl rand -hex 32` 生成；Compose `.env` 中含 `$` 等字符时注意引号和插值规则。`POSTGRES_DEV_PORT`、`API_DEV_PORT` 仅在叠加 `compose.dev.yaml` 时生效。

### 5.5 独立 Docker / 1Panel：分别配置三个容器

以下是本机 HTTP 的可执行示例，镜像需已构建或拉取。1Panel 可按相同文件内容分别填写各容器的环境变量，使用同一自定义网络 `wekeeper-standalone`。文件中的值填写实际内容，`docker run --env-file` 不会展开 `$变量`，也不要照搬 Compose 的引号写法。

**数据库：保存为 `.env.postgres`。**

```dotenv
POSTGRES_DB=wekeeper
POSTGRES_USER=wekeeper
POSTGRES_PASSWORD=replace-with-a-long-random-password
```

**后端：保存为 `.env.api`。** 密码与 `.env.postgres` 一致；若使用已有外部数据库，改为实际连接信息并跳过下面的数据库容器创建步骤。

```dotenv
WEKEEPER_BIND_ADDR=0.0.0.0:8080
WEKEEPER_DATABASE_HOST=postgres
WEKEEPER_DATABASE_PORT=5432
WEKEEPER_DATABASE_NAME=wekeeper
WEKEEPER_DATABASE_USER=wekeeper
WEKEEPER_DATABASE_PASSWORD=replace-with-a-long-random-password
WEKEEPER_DATABASE_SSL_MODE=disable
WEKEEPER_DATABASE_MAX_CONNECTIONS=10
WEKEEPER_PUBLIC_ORIGIN=http://127.0.0.1:8088
WEKEEPER_INSECURE_LOCAL_AUTH=true
WEKEEPER_WECOM_CORP_ID=
WEKEEPER_WECOM_SECRET=
WEKEEPER_WECOM_AGENT_ID=
WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS=1800
WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS=1800
WEKEEPER_WX_APP_ID=
WEKEEPER_WX_SECRET=
RUST_LOG=wekeeper_api=info,tower_http=info
```

**前端：保存为 `.env.web`。** `api` 对应下面后端容器的名称；如果修改容器名，这里也同步修改。

```dotenv
WEKEEPER_WEB_LISTEN=:80
WEKEEPER_API_UPSTREAM=api:8080
```

先创建网络和数据库，以下名称用于独立部署，已存在同名容器时不要重复执行创建：

```bash
chmod 600 .env.postgres .env.api .env.web
docker network create wekeeper-standalone
docker run -d --name postgres --network wekeeper-standalone \
  --restart unless-stopped --env-file .env.postgres \
  -v wekeeper-standalone-postgres:/var/lib/postgresql/data \
  postgres:17.4-bookworm
```

等待数据库日志显示可以接受连接，并检查就绪，再启动后端：

```bash
docker logs --tail 30 postgres
docker exec postgres sh -c 'pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB"'
# 上一条成功后继续；替换为实际发布的镜像标签。
docker run -d --name api --network wekeeper-standalone \
  --restart unless-stopped --init --env-file .env.api \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
  wekeeper-api:local
docker logs --tail 50 api
docker exec api curl --fail --silent --show-error http://127.0.0.1:8080/readyz
```

后端完成迁移且 `/readyz` 成功后，启动前端：

```bash
docker run -d --name web --network wekeeper-standalone \
  --restart unless-stopped --env-file .env.web \
  --read-only --tmpfs /tmp \
  -p 127.0.0.1:8088:80 \
  -v wekeeper-standalone-caddy-data:/data \
  -v wekeeper-standalone-caddy-config:/config \
  wekeeper-web:local
curl --fail --show-error http://127.0.0.1:8088/readyz
```

访问 `http://127.0.0.1:8088/admin`，按第 7 节通过 `docker exec -i api` 初始化管理员。这里未向宿主机开放 API 和数据库端口，前端通过容器网络访问后端。

生产独立部署时，将 `.env.api` 的 Origin 改为真实 HTTPS Origin，insecure 改为 `false`。Caddy 直接 HTTPS 则同时将 `.env.web` 的监听值改为域名，并将 Web 端口映射改为 `-p 80:80 -p 443:443`；外部 HTTPS 入口则保留 `:80`，按第 6 节方式 B 配置。

修改环境文件后，`docker restart` 不会重新读取文件：Compose 用 `up -d --force-recreate api web`，独立容器需保留命名卷并重建相应容器；只改后端变量无需重做前端镜像。

## 6. 生产服务器部署与 HTTPS

服务器至少需要 `compose.yaml`、按目标环境创建的 `.env` 和镜像仓库访问权限。
若执行 `make smoke`，还需 Makefile、tools/smoke.sh、curl 和 Python 3；不必复制旧代码目录。

### 方式 A：Caddy 直接提供 HTTPS

DNS 指向服务器，公网 80/443 可达，配置：

```dotenv
WEB_BIND_HOST=0.0.0.0
WEB_HTTP_PORT=80
WEB_HTTPS_PORT=443
WEKEEPER_WEB_LISTEN=keeper.example.com
WEKEEPER_PUBLIC_ORIGIN=https://keeper.example.com
WEKEEPER_INSECURE_LOCAL_AUTH=false
```

Caddy 自动管理证书，`caddy-data` 和 `caddy-config` 命名卷保存状态。
不要把 `localhost` 当作监听所有接口的站点地址，也不要在公共 HTTPS 环境设 insecure 为 true。

### 方式 B：Nginx / 1Panel 在同机终止 HTTPS

```dotenv
WEB_BIND_HOST=127.0.0.1
WEB_HTTP_PORT=8088
WEKEEPER_WEB_LISTEN=:80
WEKEEPER_PUBLIC_ORIGIN=https://keeper.example.com
WEKEEPER_INSECURE_LOCAL_AUTH=false
```

入口代理将该域名所有路径代理到 `http://127.0.0.1:8088`，保留 Host、Origin、Cookie 及转发协议头。
如果代理本身位于容器中，容器内的 127.0.0.1 不是宿主机；让代理与 Web 共享容器网络并使用 `web:80`，或配置明确可达的宿主机地址。
TLS 由入口平台统一管理，不同时让应用 Caddy 申请同一域名证书。
请求体限制需允许 5 MiB 图片上传；API 当前媒体上限为 5 MiB。

启动生产栈：

```bash
docker compose --env-file .env -f compose.yaml config --quiet
docker compose --env-file .env -f compose.yaml pull
docker compose --env-file .env -f compose.yaml up --no-build -d --wait --wait-timeout 180
docker compose --env-file .env -f compose.yaml ps
```

生产不叠加 `compose.dev.yaml`，不公开数据库/API 调试端口。
API 通过 outbound 网络访问官方服务，数据库只连接 internal 网络。
企业微信域名、回调、应用权限与账号映射见 [企业微信接入手册](wecom.md)。

## 7. 首次管理员初始化

无论使用 Compose 自建数据库还是已有生产数据库，首次部署都需要显式创建后台管理员；不会在生产日志中输出默认密码。下面命令创建用户名 `admin`，密码由你输入，与数据库连接密码无关。API 正常启动后，在宿主机 Bash 中执行：

```bash
read -r -s -p '管理员密码（12–128 字节）: ' WK_ADMIN_PASSWORD
printf '\n'
printf '%s' "$WK_ADMIN_PASSWORD" |
  docker compose --env-file .env -f compose.yaml exec -T api \
    /usr/local/bin/wekeeper-api --create-admin admin
unset WK_ADMIN_PASSWORD
```

使用 `admin` 和刚输入的密码登录。CLI 从标准输入读取至 EOF，创建后退出，不启动第二个 HTTP 服务。
同名账号已存在时会失败，不会重置旧密码；后续账号管理和密码重置在后台完成。
使用自定义 `-p` 项目名时，此处也加相同的 `-p`。
### 在宿主机终端执行（Bash，安装了 Docker）

独立 Docker/1Panel 的**宿主机 SSH 终端**执行下面命令，API 容器名为 `api`。不是在 1Panel 的容器终端执行：

```bash
read -r -s -p '管理员密码（12–128 字节）: ' WK_ADMIN_PASSWORD
printf '\n'
printf '%s' "$WK_ADMIN_PASSWORD" |
  docker exec -i api /usr/local/bin/wekeeper-api --create-admin admin
unset WK_ADMIN_PASSWORD
```

### 在 1Panel 的 API 容器终端执行（/bin/sh）

打开 **API 容器**的终端，选择 `/bin/sh`。此时已经在容器内，不使用 `docker exec`；容器无需安装 Docker。`read -s -p` 是 Bash 用法，不适用于这里的 `/bin/sh`。

容器网页终端可能把多行粘贴与密码输入混在一起。请按以下顺序操作，命令和密码分开输入：

1. 按 Ctrl+C 取消之前未完成的命令。若此前关闭过回显，输入 `stty echo` 并按回车；仍异常时重新打开容器终端。
2. **只输入下面这一条命令并按回车**：

```sh
/usr/local/bin/wekeeper-api --create-admin admin
```

3. 程序不会打印密码提示，正在从标准输入等待密码。此时输入新密码（12–128 字节），然后按回车。该方式下终端会显示输入内容。
4. 在新空行按 **Ctrl+D**，发送 EOF。程序读取至 EOF 后才创建账号，单独按回车不会结束输入。不要把密码与启动命令粘贴在同一行，也不要输入字面文本 `Ctrl+D`。

若网页终端不能发送 Ctrl+D，可使用一行管道（将占位内容替换为新密码，密码中不能含单引号）：

```sh
printf '%s' '替换为新的至少12字节密码' | /usr/local/bin/wekeeper-api --create-admin admin
```

一行管道中的密码可能被终端历史保存；优先使用前述标准输入方式。不要使用聊天记录中已经暴露的密码。

出现 `Administrator created: ...` 才表示创建成功。容器使用其已有数据库环境配置，将管理员写入当前连接的生产数据库。命令成功后访问生产域名的 `/admin`，使用 `admin` 和刚输入的密码登录。若提示账号已存在，使用既有管理员账号，不要重复初始化。
生产不注入 `WEKEEPER_DEV_ADMIN_PASSWORD`；嵌入式调试入口的自动管理员机制不属于 Docker 初始化流程。

## 8. 持久化与备份恢复

当前使用三个 Docker 命名卷，实际卷名前缀由 Compose project 决定（默认 `wekeeper`）：

| 卷 | 挂载位置 | 数据 |
| --- | --- | --- |
| `postgres-data` | PostgreSQL `/var/lib/postgresql/data` | 全部业务、账号、会话、审计与图片 BYTEA |
| `caddy-data` | Web `/data` | 证书等 Caddy 运行状态 |
| `caddy-config` | Web `/config` | Caddy 配置状态 |

目前不需要单独的图片目录或对象存储卷。媒体包含在数据库备份中，历史外部图片则需按迁移清单单独归档。
重启/重建容器保留卷；删除卷会删除持久数据。
更改 `.env` 中的 PostgreSQL 初始化用户名/库名/密码不会自动修改已有卷内角色与数据库。
密码轮换需先受控更新数据库角色，再更新配置并重建 API。

备份命令（确认目标环境）：

```bash
umask 077
mkdir -p backups
WK_BACKUP="backups/wekeeper-$(date +%Y%m%d-%H%M%S).dump"
docker compose --env-file .env -f compose.yaml exec -T postgres \
  sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" -Fc' > "$WK_BACKUP"
test -s "$WK_BACKUP"
sha256sum "$WK_BACKUP" > "$WK_BACKUP.sha256"
```

同时保存环境配置的受控副本、镜像 digest 和 migration 版本；备份移出单机并限制访问。
校验文件非空不等于可恢复，定期在新建隔离库演练：

```bash
# 在隔离环境执行，wekeeper_restore 必须是本次新库。
docker compose --env-file .env -f compose.yaml exec -T postgres \
  sh -c 'createdb -U "$POSTGRES_USER" wekeeper_restore'
docker compose --env-file .env -f compose.yaml exec -T postgres \
  sh -c 'pg_restore --exit-on-error --no-owner -U "$POSTGRES_USER" -d wekeeper_restore' < "$WK_BACKUP"
```

再启动隔离 API 指向恢复库，验证登录、内容、图片与分享记录。
不要覆盖正在写入的生产库；不要将 PostgreSQL 17 数据卷直接挂到其他主版本镜像。

## 9. 更新、验收与回滚

1. 保留当前镜像 digest、配置与数据库备份，审查 forward-only migration 的兼容性。
2. 在 `.env` 填写新 API/Web digest；确认 shell 没有旧的同名 export 覆盖。
3. 执行生产 pull、up，并等待就绪。
4. 检查日志和真实浏览器业务流程，记录发布结果。

```bash
docker compose --env-file .env -f compose.yaml pull
docker compose --env-file .env -f compose.yaml up --no-build -d --wait --wait-timeout 180
docker compose --env-file .env -f compose.yaml logs --tail 100 api web
curl --fail --show-error https://keeper.example.com/healthz
curl --fail --show-error https://keeper.example.com/readyz
WEKEEPER_SMOKE_ORIGIN=https://keeper.example.com make smoke
```

不使用 `curl -k` 跳过正式证书验证。
`make smoke` 验证 API/readiness 和 SPA HTML 入口，不能替代页面 JavaScript 执行验证。
浏览器验收应覆盖登录、创建/切换学校、审核内容、上传与读取图片、生成草稿和 `/wecom` 预览。
企业微信真实发送另按真机矩阵验证，普通浏览器预览不证明腾讯权限可用。
重建 API 后再检查图片和历史内容，确认持久化有效。

兼容迁移时可将 API/Web 指回旧 digest 后重新 `up --no-build -d --wait`。
已执行不兼容 migration 时不能仅回退镜像；应停写后前滚修复，或恢复与旧镜像匹配的完整备份并核对新写入数据。
已发送的外部消息不随数据库恢复而撤回，必须单独人工核对。

## 10. 独立平台部署与排障

1Panel 可以导入生产 Compose，或自行创建 API/Web/数据库容器。独立容器需显式注入数据库配置与 Web 上游，设置持久卷和网络。
Kubernetes 需要另行编写 Deployment/Service/Secret/PVC/Ingress；本仓库没有开箱可执行的 K8s 清单。
Web 使用内部 80，API 使用 8080，浏览器所有页面与 API 请求经同源 Web 入口。
API 存活探针为 `/healthz`，就绪为 `/readyz`；Web 进程检查为容器内部 `http://127.0.0.1:8081/healthz`，该端口无需公开。

```bash
docker compose --env-file .env -f compose.yaml ps
docker compose --env-file .env -f compose.yaml logs --tail 100 postgres api web
docker compose --env-file .env -f compose.yaml exec -T api \
  curl --fail --silent --show-error http://127.0.0.1:8080/readyz
docker compose --env-file .env -f compose.yaml exec -T web \
  wget -qO- http://127.0.0.1:8081/healthz
```

| 现象 | 检查方向 |
| --- | --- |
| 构建找不到 Cargo.toml 或 api-contract | 是否从仓库根目录使用 `.` 构建 |
| 无法拉取镜像 | 仓库地址、登录、标签/digest、主机 CPU 架构 |
| PostgreSQL 不健康 | 初始化变量、已有卷的真实密码、磁盘空间和日志 |
| API 启动失败/readyz 503 | 数据库连接、migration 错误；不要绕过迁移启动 |
| 页面 200 但 API 失败 | Caddy 上游、容器 DNS 与 API readiness |
| HTTP 登录失败/没有 Cookie | 本地才启用 insecure；Origin 要匹配实际浏览器地址 |
| HTTPS 登录返回 forbidden | PUBLIC_ORIGIN、代理是否保留 Origin、账号学校授权 |
| `/wecom` 显示未配置 | 企业凭据、HTTPS origin、应用成员映射；不是默认发送功能 |
| 页面加载失败 | 浏览器控制台、静态资源版本、Caddy HTML/assets 缓存；重新加载 |
| `/api/v1/posts` 返回 404 | 该无学校作用域路径不存在，使用 `/api/v1/schools/{school}/posts` |

### 当前数据库初始化基线

2026-10-31 按不保留旧数据库的方式将 14 份增量迁移合并为 4 份基线，依次执行：

| 文件 | 内容 |
| --- | --- |
| `202610310101_core_business.sql` | 元数据、身份、学校、社区、企业微信目录、公共别名、历史业务记录 |
| `202610310102_broadcast.sql` | 官方群发、帖子/纯文本来源、批次、目标和操作账本 |
| `202610310103_mobile_execution.sql` | 手机凭据、可见身份、原发送账本、规则发布、多群顺序、配对与查询索引 |
| `202610310104_automation.sql` | 多应用流程、集中维护、业务授权、v5 批次、投递与共享正文归属触发器 |

协议范围和相关约束直接按最终定义创建，删除旧回填与反复重建步骤；`maintain_delivery_content_scope` 仅在 v5 依赖表就绪后定义一次。合并前后在隔离 PostgreSQL 17 比较了 1,643 条目录记录，包含列顺序、默认值、约束、索引、视图、函数、触发器、序列归属及初始数据，结果完全一致。

新基线只用于空白 schema；不能在已有旧分段迁移账本上直接升级，不自动删除旧表、接受旧 checksum 或跳过缺失迁移。正常重启仍由 SQLx 账本校验并跳过已成功迁移；后续变更新增更大编号的前向迁移。

### 开发数据库重建与残留对象错误

`ExecuteMigration(... "check_wecom_directory_group" already exists ..., 202610200002)` 表示迁移创建函数时，当前 schema 已有同签名函数。只删除业务表不会删除独立函数或未绑定表列的序列；仅清空 `_sqlx_migrations` 也不等于重建数据库。当前迁移使用 `CREATE OR REPLACE FUNCTION` 写入本版本的函数定义，避免函数残留阻止建表。相关函数和序列的修复已包含在当前基线中，迁移编号由测试检查唯一性与顺序。

`202610200003` 中的 `relation "legacy_public_id" already exists` 同属残留对象问题：它是未归属表列的独立序列。迁移现在复用已有序列并保留计数，再用 `OWNED BY legacy_aliases.public_id` 绑定所属列；以后删除该表会一并删除序列。

本轮不提供旧分段迁移、旧 checksum 或重复编号账本的自动转换。若当前开发库已手工删表、清迁移账本，或更新后出现 checksum 不一致，应重建整个业务 schema，再由新 API 初始化。不要继续逐个删除报错对象，也不要只删迁移记录。

以下命令**会删除 Compose 专用 PostgreSQL 中 `public` schema 的全部数据、表、函数、迁移记录和相关依赖**，仅用于允许丢弃全部数据的开发环境。外部数据库、共享 schema 或需要保留数据的环境不使用这些命令。先重新构建/拉取修复后的 API 镜像，再执行：

```bash
# WeKeeper 根目录；先准备修复后的 API 镜像
make reset-dev-database
```

该入口执行 `tools/reset-dev-database.sh`：停止 API/Web，启动专用 PostgreSQL，使用容器内的数据库名和角色执行 `tools/reset-dev-schema.sql`，再启动 API/Web 并检查就绪。SQL 在一个事务内删除并重建 `public`，同时清除表、函数、序列和 `_sqlx_migrations`。任一步失败立即停止，避免在未重建成功时继续启动。默认读取 `.env`；其他 Compose 环境文件可用 `WEKEEPER_COMPOSE_ENV_FILE=/path/to/dev.env make reset-dev-database` 指定。

没有执行自动清库；API 正常启动仍只执行迁移。上述命令保留 PostgreSQL 数据卷和 Caddy 证书卷，但业务表中的媒体也会随 schema 清除。外部 PostgreSQL 的一次性开发库由其管理员重建专用数据库/schema，应用连接角色须拥有新 schema 的建表权限。

本文按仓库 Dockerfile、Compose、Caddy 和配置解析代码核对。镜像构建、Compose 就绪及公网 HTTPS 仍需在具备 Docker daemon 的目标环境实测。
