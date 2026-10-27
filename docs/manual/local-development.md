# 本地开发与凭据注入

命令默认在 WeKeeper 仓库根目录运行。工程固定 Rust 1.97.1、Node 22、pnpm 11.22.0，可使用 devbox；首次需要下载工具链和依赖。

## 嵌入式 PostgreSQL

不需要 Docker 或手工创建数据库：

```bash
make install
pnpm --dir api-contract build
devbox run -- make dev-embedded
```

另一个终端：

```bash
pnpm --dir frontend dev
```

前端 `http://localhost:5173`，代理 API 至 `127.0.0.1:8080`；后台 `/admin`、社区 `/community`、企业微信页面 `/wecom`。Vite 端口固定，占用直接报错，代理目标通过 `WEKEEPER_API_ORIGIN` 设置。

嵌入式入口准备临时 PostgreSQL 17，忽略外部数据库连接配置，退出删除临时数据。每次重启数据重置，自动生成 admin 密码并在终端显示；可通过 `WEKEEPER_DEV_ADMIN_PASSWORD` 安全注入 12–128 字节密码，自定义密码不输出。不把此机制用于生产。

默认 public origin 为 `http://localhost:5173`。本地 HTTP 模式允许同端口的 localhost、127.0.0.1 和 IPv6 loopback；使用局域网地址或不同端口时，后端启动前设置对应的 `WEKEEPER_PUBLIC_ORIGIN`，避免 Origin 检查失败。

Devbox 初始化和嵌入式 Make 入口会修复项目本地 Rust 链接器旧路径。迁移项目目录后仍出现旧 gcc-ld/ld.lld 错误时执行：

```bash
python3 tools/repair_rustup_linker.py
```

## 企业微信凭据注入

嵌入式入口继承启动终端导出的变量，不自动读取 `.env`。Secret 只注入 API，不传给 Vite，不写成 `VITE_*`：

```bash
export WEKEEPER_WECOM_CORP_ID='替换为企业 CorpID'
read -rs WEKEEPER_WECOM_SECRET
export WEKEEPER_WECOM_SECRET
export WEKEEPER_WECOM_AGENT_ID=''
devbox run -- make dev-embedded
```

目录同步与成员绑定可留空 AgentID；填写时与 Secret 对应同一自建应用。需填写真实 AgentID 的功能按 [企业微信接入手册](../runbooks/wecom.md) 配置。真实凭据会触发官方接口访问，还需出口可信 IP 和应用可见范围。

可创建已忽略的 `.env.wecom.local`（合法 shell 赋值，不提交）：

```dotenv
WEKEEPER_WECOM_CORP_ID='替换为企业 CorpID'
WEKEEPER_WECOM_SECRET='替换为应用 Secret'
WEKEEPER_WECOM_AGENT_ID=''
```

显式在子 shell 加载，避免污染其他终端：

```bash
chmod 600 .env.wecom.local
(
  set -a
  . ./.env.wecom.local
  set +a
  devbox run -- make dev-embedded
)
```

更改变量后重启 API。嵌入式入口强制本地 insecure 登录，OAuth 始终关闭；OAuth 使用普通 API/Compose 的 HTTPS 环境验收。群发独立受 `WEKEEPER_WECOM_BROADCAST_ENABLED` 控制，配置完整并显式开启会访问真实平台，不能把本地 origin 当作模拟发送开关。

## 外部数据库开发

保留开发数据时使用独立 PostgreSQL，先按 [Docker 手册](../runbooks/docker.md) 创建环境文件和测试数据库，再启动数据库服务：

```bash
make install
pnpm --dir api-contract build
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up -d postgres
```

在启动 API 的终端注入实际数据库变量，密码不写入命令历史：

```bash
export WEKEEPER_DATABASE_HOST=127.0.0.1
export WEKEEPER_DATABASE_PORT=5432
export WEKEEPER_DATABASE_NAME=wekeeper
export WEKEEPER_DATABASE_USER=wekeeper
read -rs WEKEEPER_DATABASE_PASSWORD
export WEKEEPER_DATABASE_PASSWORD
export WEKEEPER_PUBLIC_ORIGIN=http://localhost:5173
export WEKEEPER_INSECURE_LOCAL_AUTH=true
devbox run -- make api-dev
```

`WEKEEPER_DATABASE_PASSWORD` 需要与数据库实际密码一致。本地 API 不自动读 `.env`，Compose 的 env 注入不等于本地终端已配置。另一个终端运行 `pnpm --dir frontend dev`。普通 API 不自动创建临时管理员，初始化步骤见 [Docker/管理员手册](../runbooks/docker.md)。

## 检查与限制

运行 [开发检查命令](../development/index.md#应用检查)。数据库集成测试只用 `WEKEEPER_TEST_DATABASE_URL` 或既有嵌入式隔离入口，不回退生产地址。系统 socket、数据库二进制下载和 Docker 访问受限时如实记录，不能把静态配置检查替代实际启动。
