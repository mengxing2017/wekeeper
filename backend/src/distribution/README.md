# 模板与企业统一群发

状态：已实现并在 bootstrap/http 装配。运行和验收见 docs/runbooks/business.md。

职责：模板版本、群发草稿快照、按群主分批、单次外部调用、结果对账与人工核对；旧分享记录只保留历史语义。

约束：创建成功不等于送达，成员仍需确认；unknown 不自动重试；学校授权、当前群主及凭据代次必须有效。

模块边界：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

业务与权限回归位于 backend/tests/business.rs，共享 client 位于 api-contract/src/modules/business。

群发入口 north/broadcast.rs、north/broadcast_remote.rs；端口 south/port/broadcast.rs；事务 south/adapter/postgres/broadcast.rs；测试 backend/tests/broadcast.rs。
