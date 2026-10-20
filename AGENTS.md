# WeKeeper

新架构只有两个部署应用：`backend/`（Rust）与 `frontend/`（React）。
`api-contract/` 是不部署的 TypeScript 类型与 client 包。
旧 PHP、uni-app、Java 与历史资源保留供迁移核对，不在新 Docker 镜像中构建或运行。

## 边界

- Rust 按业务模块划分 `domain/`、`north/local/`、`north/remote/`、`south/port/`、`south/adapter/postgres/`。
- HTTP 层不写 SQL；业务规则不依赖 Axum、SQLx。
- SQLx migration 放在 `backend/migrations/`，递增且 forward-only；启动时自动迁移。
- 前端按 `features/admin/`、`features/operations/` 组织；`shared/` 不保存接口 DTO/client。
- 只在 feature 的 `api/index.ts` 引用 `@wekeeper/api-contract/*`，不得另写相同 DTO 或绕过 client 请求。
- 公开接口修改同步更新后端、contract 与契约测试。TS 编译不能替代 Rust 序列化契约验证。
- 未完成身份与学校授权前不得暴露业务读写接口；当前仅开放健康与无敏感信息的应用说明。
- 不接入千寻、Hook、Binlog 发送；发送流程后续按 report/11 的人工确认与 unknown 语义实现。

## 验证

```bash
make check-web
devbox run -- make check-backend
```

数据库集成测试只接受 `WEKEEPER_TEST_DATABASE_URL`，不得回退生产地址。
本地无 Docker 调试使用 `devbox run -- make dev-embedded`，自动创建临时 PostgreSQL 17。
嵌入式入口忽略外部数据库配置，退出删除数据；`devbox run -- make test-embedded` 验证生命周期。
部署入口为 `compose.yaml`、`compose.dev.yaml` 与 `env.example`，仅包含 web、api、postgres。
详细设计见 `report/11-Rust与React重构实施方案.md`，运行方法见根 `README.md`。
