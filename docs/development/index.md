# 开发入口

## 目录与边界

| 目录 | 职责 |
| --- | --- |
| `backend/` | Rust API；domain、north/local、north/remote、south/port、south/adapter/postgres |
| `frontend/apps/web/` | React Web，Caddy 部署 |
| `frontend/apps/miniapp/` | uni-app Vue 3 小程序，上传微信平台 |
| `api-contract/` | 统一 DTO/JSON:API/client，不部署 |
| `docs/` | 独立 VitePress 文档包，独立构建的静态资料，随 Web 镜像发布 |
| `tools/` | 迁移、检查、冒烟及开发工具 |

旧 PHP、根 uniapp Vue 2、Java 仅作迁移参考，不参与新镜像。HTTP 层不写 SQL；公共接口变更同步 Rust、contract 和契约测试。客户端边界详见 [前端说明](./frontend.md)。

## 文档站

只需 Node 22、pnpm 11.22.0。在仓库根目录运行：

```bash
make docs-install
make docs
```

| Make 命令 | 等价 pnpm 命令 | 结果 |
| --- | --- | --- |
| `make docs-install` | `pnpm --dir docs install --frozen-lockfile` | 安装独立文档依赖 |
| `make docs` | `pnpm --dir docs dev` | 热更新，127.0.0.1:22596/docs/ |
| `make docs-build` | `pnpm --dir docs build` | 静态站，docs/.vitepress/dist |
| `make docs-preview` | `pnpm --dir docs preview` | 构建后预览，127.0.0.1:22597/docs/ |

先构建再 preview。端口占用明确报错；换端口可执行 `pnpm --dir docs dev --port 22598`。需要局域网浏览时显式添加 `--host 0.0.0.0` 并核对可访问范围。

`listen EPERM` 表示运行环境禁止监听本地 socket，应在允许本地服务的终端启动；它与端口占用 `EADDRINUSE` 不同，换端口不能解决权限限制。

文档独立安装，不加入 frontend/api-contract 工作区，静态产物进入 Web 镜像，不新增 Compose 服务。pnpm 配置只授权 esbuild 必要构建脚本，不开启所有依赖脚本。新增主题在 `.vitepress/config.ts` 中登记，ADR/runbooks 原路径保留；跨站本机绝对路径不放进网页链接。

## 应用检查

```bash
make install
make check-web
devbox run -- make check-backend
make check-miniapp
```

`make check-web` 已覆盖 contract、Web/小程序类型、lint、测试和构建；`make check-miniapp` 可单独核验小程序。后端检查含格式、构建、Clippy 和普通测试。纯文档修改执行 `make docs-build`，不需要启动业务服务。

数据库集成测试另行执行：只接受隔离 `WEKEEPER_TEST_DATABASE_URL`。`make dev-embedded` / `make test-embedded` 使用项目既有临时 PostgreSQL 生命周期入口，命令见 [本地开发](../manual/local-development.md)。不连接生产库验证文档。


## 按开发任务查阅

- [Rust 后端与设备 WebSocket](backend.md)
- [协议、身份与学校授权](contracts-and-auth.md)
- [测试与验收](testing.md)
- [文档写作与发布](docs-publishing.md)

业务使用者请从[使用手册](../guide/index.md)开始。
