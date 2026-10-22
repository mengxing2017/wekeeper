# 官方平台适配

状态：已实现并在 bootstrap/http 装配。运行和验收见 docs/runbooks/business.md。

职责：微信与企业微信的 OAuth、token 与官方 API 适配。

约束：只接官方能力；密钥、票据及认证 URL 不写日志，不接入 Hook。

模块边界：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

业务与权限回归位于 backend/tests/business.rs，共享 client 位于 api-contract/src/modules/business。

通讯录：`north/directory.rs` 编排同步与核验；`south/port` 隔离官方 client 和仓储；`directory_http` 处理凭据、错误码及读取重试，`directory_postgres` 处理租约、原子发布和绑定事务。
任务在 backend runtime 中运行，不增加部署应用。数据库集成回归见 `backend/tests/directory.rs`。

群发 HTTP adapter 为 south/adapter/broadcast_http.rs，读取可重试，外部写入禁止重试及重定向。已删除 JS-SDK 签名/票据实现，兼容路由返回停用错误。
