# WeKeeper

校园社区与企业微信内容运营重构工程。新应用采用 Rust + React。

现已接入身份与学校权限、社区内容和互动、审核治理、运营配置、模板版本、分享草稿、企业微信人工分享及记录、小程序兼容和数据迁移工具。
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
前端接口调用通过 `@wekeeper/api-contract/*`；后台与 H5 懒加载隔离，H5 不加载 Ant Design 后台入口。

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
后者包含 Rust 格式、Clippy 与不依赖数据库的 HTTP/契约测试。
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

## 生产发布

构建上下文必须是仓库根目录；`.dockerignore` 仅允许三个新应用进入上下文，排除历史凭证、归档和二进制资源。

```bash
docker build -f backend/Dockerfile -t your-registry/wekeeper-api:your-version .
docker build -f frontend/Dockerfile -t your-registry/wekeeper-web:your-version .
docker push your-registry/wekeeper-api:your-version
docker push your-registry/wekeeper-web:your-version
```

在服务器 `.env` 设置 `API_IMAGE`、`WEB_IMAGE` 为已发布的镜像，优先固定 digest。
配置强数据库密码、公开域名与 TLS 入口，再执行：

```bash
docker compose --env-file .env -f compose.yaml pull
docker compose --env-file .env -f compose.yaml up -d --wait
```

完整的 HTTPS、网络、备份和更新步骤见 [Docker 部署手册](docs/runbooks/docker.md)。
工程决策见 [基础骨架 ADR](docs/adr/0001-foundation.md)。
业务目标见 [Rust 与 React 重构实施方案](report/11-Rust与React重构实施方案.md)。

首次管理员初始化、业务操作、平台配置、旧数据迁移和增量追平命令详见 [业务运行手册](docs/runbooks/business.md)。
