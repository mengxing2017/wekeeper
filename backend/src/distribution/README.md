# 模板与人工分享

状态：已实现并在 bootstrap/http 装配。运行和验收见 docs/runbooks/business.md。

职责：模板版本、正文快照、草稿、发送尝试与人工核对。

约束：SDK 成功不等于送达；unknown 不自动重试，目标必须绑定学校与当前客户群。

模块边界：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

业务与权限回归位于 backend/tests/business.rs，共享 client 位于 api-contract/src/modules/business。
