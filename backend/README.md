# WeKeeper API

Rust 模块化单体，Axum HTTP 网关，SQLx PostgreSQL 适配器。

HTTP 请求资源、成功文档及错误文档复用 `cairnkit-jsonapi`，通过 HTTPS Git 依赖固定到 `1068bcbc04750a1d5d6d63a429260237e8a19861`；Cargo.lock 一并锁定，最低 Rust 版本为 1.97.1。首次构建需要访问该仓库及 crates.io，Docker 构建使用相同依赖，不需要本地源码副本。

本地 `http/error.rs` 仅负责业务错误到状态码/消息的映射、Axum 拒绝响应适配；错误对象及序列化由 Cairnkit 提供。成功响应、资源模型、写入文档与分页均通过 `http/jsonapi.rs` 适配到 Cairnkit，业务层保持独立。所有结构化业务响应使用 application/vnd.api+json；旧 data 包装及 /api/discover 表单兼容层已删除。详细 wire 契约见 `../api-contract/README.md`。SQLx/PostgreSQL 迁移继续由原迁移机制管理，未替换为 Cairnkit 的 SQLite/Refinery runner。
system 是已接线的完整分层样板；其他模块 README 记录后续实施边界。
bootstrap 装配服务，http 组合路由，domain 保持独立于传输与数据库。

启动需要 WEKEEPER_DATABASE_URL 或数据库分字段环境变量；详见根 README。
执行 migration 成功后监听端口，SIGINT/SIGTERM 触发优雅停止。
无 Docker 调试可用 `devbox run -- make dev-embedded`：自动启动临时 PostgreSQL 17 并运行相同 API。
示例入口在 `examples/embedded_postgres.rs`，临时库退出删除，二进制安装缓存保留；不会读取外部数据库配置。
`runtime` 共享迁移、监听与停止逻辑，避免调试入口和正式入口运行不同代码路径。
healthz 不访问数据库；readyz 查询基础 schema，最多等待 2 秒。
未知业务路径返回 JSON 404；没有默认用户、认证旁路或真实发送功能。

```bash
devbox run -- make check-backend
devbox run -- make test-integration
```

第二条要求调用者显式提供隔离的 WEKEEPER_TEST_DATABASE_URL。
