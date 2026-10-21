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
构建机需访问基础镜像仓库、crates.io、npm 和 Debian 包源；仅运行镜像的服务器无需安装 Rust、Node、pnpm。

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
`--pull` 更新基础镜像缓存，不保证可重现的基础镜像 digest；有严格重现要求时需另行固定 Dockerfile 基础镜像摘要。

API 构建通过 BuildKit cache mount 缓存 Cargo registry 和 target；同一 builder 可复用。
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

## 5. 运行配置

环境变量由 Compose 注入，后端二进制不自动读取 `.env`。不要将业务秘密放入 Dockerfile 或构建参数。
Shell 中已 export 的同名变量优先于 `.env`；从打包终端部署不同环境前，清理/核对 `API_IMAGE`、`WEB_IMAGE` 等覆盖值。

### 当前 Compose 支持的配置

| 变量 | 默认值/示例 | 用途 |
| --- | --- | --- |
| `API_IMAGE` / `WEB_IMAGE` | `wekeeper-api:local` / `wekeeper-web:local` | 应用镜像；生产改成发布版本或 digest |
| `POSTGRES_IMAGE` | `postgres:17.4-bookworm` | 数据库镜像 |
| `POSTGRES_DB` / `POSTGRES_USER` | `wekeeper` | 初始化库与角色，API 同时使用 |
| `POSTGRES_PASSWORD` | 必填 | 数据库及 API 连接密码 |
| `WEB_BIND_HOST` | `127.0.0.1` | 宿主机 Web 端口绑定地址 |
| `WEB_HTTP_PORT` / `WEB_HTTPS_PORT` | `8088` / `8443` | 映射至容器 80/443 |
| `WEKEEPER_WEB_LISTEN` | `:80` | Caddy 站点地址；直接 TLS 时填写域名 |
| `WEKEEPER_PUBLIC_ORIGIN` | `http://127.0.0.1:8088` | 浏览器精确 origin，不带路径或末尾斜杠 |
| `WEKEEPER_INSECURE_LOCAL_AUTH` | `false` | 本地 HTTP 登录设 true；生产 HTTPS 保持 false |
| `WEKEEPER_DATABASE_MAX_CONNECTIONS` | `10` | 连接池上限，允许 1–100 |
| `WEKEEPER_WECOM_CORP_ID` / `WEKEEPER_WECOM_SECRET` | 空 | 服务端企业微信凭据，目录、成员核验与 OAuth 必需 |
| `WEKEEPER_WECOM_AGENT_ID` | 空 | 当前客户端分享必需的正整数应用 ID；纯服务端场景可留空，须与 Secret 属于同一应用 |
| `WEKEEPER_WX_APP_ID` / `WEKEEPER_WX_SECRET` | 空 | 小程序凭据 |
| `RUST_LOG` | `wekeeper_api=info,tower_http=info` | API 日志级别 |
| `POSTGRES_DEV_PORT` / `API_DEV_PORT` | `5432` / `8080` | 仅开发 override 使用的回环端口 |

数据库密码使用分项环境变量传入，无需 URL 编码；包含 `$` 等字符时注意 Compose `.env` 引号/插值规则。
可用 `openssl rand -hex 32` 生成便于配置的随机密码。

### 独立容器与外部数据库

API 二进制还支持 `WEKEEPER_BIND_ADDR`、`WEKEEPER_DATABASE_URL`、`WEKEEPER_DATABASE_HOST`、
`WEKEEPER_DATABASE_PORT`、`WEKEEPER_DATABASE_NAME`、`WEKEEPER_DATABASE_USER`、
`WEKEEPER_DATABASE_PASSWORD`、`WEKEEPER_DATABASE_SSL_MODE`。
完整 URL 一旦设置会优先使用；不使用 URL 时应移除该变量，不要填写空字符串。
分项默认 SSL 模式为 `prefer`；生产远程数据库按证书条件使用 `verify-full`，并配置受信任 CA。

**当前 compose.yaml 固定连接 `postgres:5432`、SSL disable，并设置 Web 上游为 `api:8080`。**
仅在 `.env` 写 `WEKEEPER_DATABASE_HOST` 或 `WEKEEPER_API_UPSTREAM` 不会改变该文件中的固定值。
采用外部数据库或自定义上游时，需要自建部署编排，显式修改 API environment、数据库依赖与 Web 上游。
API 镜像不支持参考项目中的 OTA、地图、会话密钥等配置，不要照搬 `OPS_*` 字段。

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

Docker 没有预设账号或密码。API 正常启动后，在宿主机 Bash 中执行：

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
独立 Docker/1Panel 可将 Compose exec 替换成 `docker exec -i <API容器名>`。
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

本文按仓库 Dockerfile、Compose、Caddy 和配置解析代码核对。镜像构建、Compose 就绪及公网 HTTPS 仍需在具备 Docker daemon 的目标环境实测。
