# WeKeeper

校园社区与企业微信内容运营重构工程。新应用采用 Rust + React。

现已接入身份与学校权限、社区内容和互动、审核治理、运营配置、模板版本、分享草稿、企业微信人工分享及记录、小程序身份入口和数据迁移工具。
运行与数据切换见 [业务运行手册](docs/runbooks/business.md)。真实平台权限、真机及生产数据切换仍需验收。
旧 PHP、uni-app、Java 和附带资源保留用于迁移核对，新容器不会运行它们。

## 目录

```text
backend/         Rust / Axum / SQLx，部署为 api
frontend/        React / TypeScript / Vite，Caddy 部署为 web
api-contract/    共享 DTO 与请求 client，不部署
docs/adr/        工程决策
docs/runbooks/   部署、验证与恢复操作
tools/           冒烟检查与后续迁移工具
report/          原系统分析及重构实施方案
```

后端以 `system` 模块展示完整的 domain、north/local、north/remote、south/port、south/adapter/postgres 分层。
业务模块按用例、领域与 PostgreSQL 端口实现；审核事务归入 community。
前端接口调用通过 `@wekeeper/api-contract/*`；后台、社区与企业微信页面按入口懒加载，使用统一主题。

## Docker 文档导航

- [完整镜像打包、发布与部署手册](docs/runbooks/docker.md)：本地构建、仓库发布、离线交付、HTTPS、初始化、备份与回滚。
- [企业微信配置与验收](docs/runbooks/wecom.md)：可信域名、OAuth、草稿预览及真机验证。
- [业务运行与数据迁移](docs/runbooks/business.md)：账号、学校、审核、历史数据导入与切换。

部署单元为 `api`、`web`、`postgres`；`api-contract` 参与构建但不单独运行。
`compose.yaml` 运行已发布镜像，`compose.dev.yaml` 才提供源码构建配置。

## Docker 本地启动

需要 Docker Engine 与 Compose v2；首次构建需要访问 Docker Hub、crates.io 和 npm。

```bash
cp env.example .env
# 编辑 .env，替换 POSTGRES_PASSWORD；本地 HTTP 设置 WEKEEPER_INSECURE_LOCAL_AUTH=true。
# WEKEEPER_PUBLIC_ORIGIN 必须与浏览器访问地址一致；不要提交 .env。
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml config --quiet
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up --build --wait
```

默认访问地址：

- 后台：http://127.0.0.1:8088/admin
- 社区：http://127.0.0.1:8088/community
- 企业微信 H5 页面：http://127.0.0.1:8088/wecom
- API：http://127.0.0.1:8088/api/v1/system
- 存活检查：http://127.0.0.1:8088/healthz
- 就绪检查：http://127.0.0.1:8088/readyz

`/readyz` 实际检查 PostgreSQL 与初始化 schema；数据库不可用时返回 503。
迁移失败时 api 启动失败，不会以“健康”状态接收请求。
H5 配置平台凭据并通过 OAuth 后，读取当前客户群，人工确认调用官方 SDK；客户端报告成功不等于平台送达证明。
Docker 不自动创建管理员；启动成功后按下方“首次创建管理员”执行。

```bash
make smoke
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml logs -f api web
make compose-down
```

`compose-down` 保留持久卷。不要对有真实数据的环境执行 `down --volumes`。
`compose.dev.yaml` 将数据库和 API 端口绑定在 127.0.0.1，便于本地调试。
生产只使用 `compose.yaml`，不直接公开 PostgreSQL/API 端口。

## 本地源码开发

工程固定 Rust 1.97.1、Node.js 22、pnpm 11.22.0，可使用已配置的 Devbox。
旧根 package.json 属于 FastAdmin；不要在根目录执行 npm install 启动新前端。
子项目 `.npmrc` 显式使用 HTTPS npm 源与证书校验，避免继承旧根配置的 HTTP 镜像。

### 嵌入式 PostgreSQL（推荐本地调试）

不需要 Docker，也不需要手动安装、创建或配置数据库。执行：

```bash
make install
pnpm --dir api-contract build
devbox run -- make dev-embedded
```

也可使用 `devbox run dev-embedded`，或执行：

```bash
devbox run -- cargo run --locked --manifest-path backend/Cargo.toml --example embedded_postgres
```

该开发入口自动准备 PostgreSQL 17、选择随机数据库端口、创建临时 `wekeeper_dev` 数据库，
执行与正式 API 相同的 migration，然后在 `127.0.0.1:8080` 启动 API。
首次运行可能下载数据库二进制，后续复用用户目录 `~/.theseus/postgresql` 的安装缓存。
数据库使用随机密码；日志不输出密码或连接 URL。
此入口忽略 `WEKEEPER_DATABASE_*`，不会连接 `.env` 或环境变量中的外部数据库。
通过 `WEKEEPER_BIND_ADDR=127.0.0.1:其他端口` 修改 API 端口，只允许环回地址。
Ctrl+C 或 SIGTERM 正常退出时，关闭连接、停止数据库并删除临时数据；**每次重启数据都会重置**。
强制终止进程、系统崩溃等情况下不保证完成清理。需要持久数据时使用下面的外部数据库方式。
嵌入式依赖只属于 Cargo dev-dependencies，生产 Docker 仍连接独立 PostgreSQL。
启动时自动创建本地管理员 `admin`，生成随机密码并在终端显示。可设置 `WEKEEPER_DEV_ADMIN_PASSWORD`（12–128 字节）自定义密码，自定义密码不会输出到终端。嵌入式入口自动启用本地 HTTP 登录，默认前端地址为 `http://localhost:5173`，可通过 `WEKEEPER_PUBLIC_ORIGIN` 修改。本地 HTTP 登录模式允许同协议、同端口的 `localhost`、`127.0.0.1` 与 `[::1]` 地址；使用其他端口时须设置匹配的 `WEKEEPER_PUBLIC_ORIGIN`。每次重启数据库与随机密码都会重置；这些自动初始化行为仅用于嵌入式开发入口。

另一个终端运行 `pnpm --dir frontend dev` 即可联调。

### 外部 PostgreSQL / Docker 数据库（保留开发数据）

```bash
make install
pnpm --dir api-contract build
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up -d postgres
```

一个终端设置数据库连接并启动 Rust：

```bash
export WEKEEPER_DATABASE_HOST=127.0.0.1
export WEKEEPER_DATABASE_PORT=5432
export WEKEEPER_DATABASE_NAME=wekeeper
export WEKEEPER_DATABASE_USER=wekeeper
# 将 .env 中的密码安全地注入 WEKEEPER_DATABASE_PASSWORD。
devbox run -- make api-dev
```

API 默认监听 8080；另一个终端运行：

```bash
pnpm --dir frontend dev
```

Vite 默认监听所有网络接口，固定使用 5173 端口（占用时直接报错），支持通过 `http://localhost:5173`、`http://127.0.0.1:5173` 和 `http://<本机局域网IP>:5173` 访问，启动时显示 Local / Network 地址。将 `/api`、`/healthz`、`/readyz` 代理至 8080。
通过局域网 IP 登录时，后端启动需设置对应的浏览器来源，例如 `WEKEEPER_PUBLIC_ORIGIN=http://192.168.1.100:5173 make dev-embedded`（替换为实际 IP），以通过 Origin 校验。后端仍可监听回环地址，由 Vite 代理请求。
通过 `WEKEEPER_API_ORIGIN` 修改 Vite 的 API 代理目标。
也可使用 `WEKEEPER_DATABASE_URL` 代替分字段配置；URL 中的密码需要 URL 编码。
后端不自动读取 `.env` 文件；Compose 会注入环境变量，本地进程由调用者注入。
连接远程数据库时配置 `WEKEEPER_DATABASE_SSL_MODE=verify-full` 和受信任 CA，或在 URL 中声明 TLS 参数。

## 质量检查

单独检查共享接口包：

```bash
pnpm --dir api-contract build
```

该命令执行 TypeScript 类型检查，不生成打包产物；完整前端检查也包含此步骤。

```bash
make check-web
devbox run -- make check-backend
```

前者包含 contract 类型/测试、React 类型检查、ESLint、行为测试与生产构建。
后者包含 Rust 格式、普通构建、Clippy 与不依赖数据库的 HTTP/契约测试。
数据库测试默认忽略，必须明确提供可丢弃的测试数据库后执行：

```bash
export WEKEEPER_TEST_DATABASE_URL='postgres://test-user:test-password@127.0.0.1:5432/wekeeper_test'
devbox run -- make test-integration
```

不会回退使用生产 `WEKEEPER_DATABASE_URL`。
嵌入式开发入口还提供独立生命周期检查（需要本地端口与首次下载权限）：

```bash
devbox run -- make test-embedded
```

验证迁移/就绪、忽略外部数据库配置、Ctrl+C/SIGTERM 退出及 API 端口冲突后的数据库清理。
信号生命周期自动检查面向 Linux/macOS 等 POSIX 环境。
CI 额外启动隔离 PostgreSQL，验证 migration 可重复执行，并构建 Docker 三服务执行 HTTP 冒烟测试。
当前状态接口的 JSON 样例同时由 Rust HTTP 测试与 TypeScript client 测试核对。
本次本地执行结果与容器验证限制见 [骨架交付验证记录](docs/runbooks/foundation-validation.md)。

## Docker 镜像打包与发布

在安装 Docker Engine、Compose v2 和 BuildKit 的构建机执行。运行服务器仅需 Docker 与配置文件，不需要源码工具链。
先完成上面的质量检查，然后选择与源码 commit 对应的版本标签：

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

将示例仓库和版本替换为实际值。**构建上下文必须是仓库根目录 `.`**，前端打包依赖同仓库的 `api-contract/`。
后端使用 Rust 多阶段构建及 Cargo 缓存，运行镜像为非 root；Web 镜像使用 Caddy 提供 React 页面与同源反向代理。
`.dockerignore` 排除历史目录、环境文件、node_modules 和本地编译产物。
发布记录保存 commit、镜像 digest、构建时间与 migration 版本；生产优先按 digest 固定镜像。

```bash
docker image inspect "$API_IMAGE" --format '{{json .RepoDigests}}'
docker image inspect "$WEB_IMAGE" --format '{{json .RepoDigests}}'
```

离线 `docker save/load`、跨架构 Buildx 及缓存说明见 [完整打包手册](docs/runbooks/docker.md#4-构建并发布版本镜像)。

## 生产部署

将 `compose.yaml`、`env.example` 交付到服务器，首次创建 `.env` 并限制权限：

```bash
cp env.example .env
chmod 600 .env
```

已有 `.env` 时直接编辑。至少配置发布的 `API_IMAGE`、`WEB_IMAGE`、强 `POSTGRES_PASSWORD`、公开 origin 和 TLS 入口。

| 部署方式 | 关键配置 |
| --- | --- |
| 本地 HTTP 调试 | `WEKEEPER_WEB_LISTEN=:80`、`WEKEEPER_PUBLIC_ORIGIN=http://127.0.0.1:8088`、`WEKEEPER_INSECURE_LOCAL_AUTH=true` |
| Caddy 直接提供 HTTPS | `WEB_BIND_HOST=0.0.0.0`、HTTP/HTTPS 端口 `80/443`、`WEKEEPER_WEB_LISTEN=keeper.example.com`、`WEKEEPER_PUBLIC_ORIGIN=https://keeper.example.com`、insecure 为 `false` |
| 同机 Nginx / 1Panel 提供 HTTPS | Web 保持 `127.0.0.1:8088`、`WEKEEPER_WEB_LISTEN=:80`，入口代理至 Web；public origin 填真实 HTTPS 域名、insecure 为 `false` |

所有域名替换为实际值；Caddy 直接 TLS 需 DNS 和公网 80/443 就绪。
配置企业微信/小程序时填写相应凭据；本地预览不要求这些凭据。
当前 Compose 固定连接内置 `postgres` 与 `api:8080`，外部数据库或自定义上游需要调整编排，不能只在 `.env` 加同名变量。
完整变量表见 [运行配置](docs/runbooks/docker.md#5-运行配置)。

```bash
docker compose --env-file .env -f compose.yaml config --quiet
docker compose --env-file .env -f compose.yaml pull
docker compose --env-file .env -f compose.yaml up --no-build -d --wait --wait-timeout 180
docker compose --env-file .env -f compose.yaml ps
```

生产仅使用 `compose.yaml`。Shell 中已 export 的镜像变量会覆盖 `.env`，部署前确认没有遗留构建环境的值。

### 首次创建管理员

Docker 没有默认账号密码，以下在宿主机 Bash 执行，密码不回显：

```bash
read -r -s -p '管理员密码（12–128 字节）: ' WK_ADMIN_PASSWORD
printf '\n'
printf '%s' "$WK_ADMIN_PASSWORD" |
  docker compose --env-file .env -f compose.yaml exec -T api \
    /usr/local/bin/wekeeper-api --create-admin admin
unset WK_ADMIN_PASSWORD
```

使用 `admin` 和刚输入的密码登录。同名账号已存在时 CLI 不会覆盖旧密码。
生产不设置 `WEKEEPER_DEV_ADMIN_PASSWORD`；嵌入式开发的自动管理员流程不适用于 Docker。

### 验收、备份与更新

```bash
WEKEEPER_SMOKE_ORIGIN=https://keeper.example.com make smoke
docker compose --env-file .env -f compose.yaml logs --tail 100 api web
```

仅交付 Compose 文件的服务器可直接用 curl 检查 `/healthz` 和 `/readyz`；`make smoke` 额外需要 Makefile、tools/smoke.sh、curl 和 Python 3。
冒烟检查 SPA HTML 不能证明页面已渲染，还需在浏览器验证登录、学校、审核、图片及 `/wecom` 草稿预览。

业务与媒体图片保存在 `postgres-data` 命名卷，证书状态保存在 `caddy-data` / `caddy-config`。
更新前保存数据库备份与旧镜像 digest；修改镜像引用后重新 pull、up 并验收。
不要使用 `down --volumes` 清理有数据的部署；forward-only migration 需要单独评估旧镜像兼容性。
完整备份、隔离恢复、更新回滚和排障命令见 [Docker 部署手册](docs/runbooks/docker.md)。

工程决策见 [基础骨架 ADR](docs/adr/0001-foundation.md)。
业务目标见 [Rust 与 React 重构实施方案](report/11-Rust与React重构实施方案.md)。
首次管理员、平台配置与旧数据迁移详见 [业务运行手册](docs/runbooks/business.md)。

### JSON:API 协议切换

所有 `/api/v1` 结构化业务请求/响应统一使用 `application/vnd.api+json`；成功资源使用 `type/id/attributes`，错误使用 `errors[]`。分页统一为 `page[number]`、`page[size]` 和 `meta.page.has_more`，前端与后端需同时更新。旧 `/api/discover/*` 表单业务兼容接口已删除；旧客户端需迁移到 api-contract，历史图片读取及数据迁移映射保留。健康探针、二进制图片和 OAuth 重定向维持各自传输用途。详见 [API contract](api-contract/README.md)。
