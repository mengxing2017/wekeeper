# 身份与会话

状态：已实现并在 bootstrap/http 装配。运行和验收见 docs/runbooks/business.md。

职责：管理员、社区用户、会话、平台身份交换及认证策略。

约束：学校权限由 schools 配合；业务写入接入前必须完成会话、CSRF 与 Origin 校验。

模块边界：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

业务与权限回归位于 backend/tests/business.rs，共享 client 位于 api-contract/src/modules/business。
