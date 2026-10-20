# 学校与授权

状态：已实现并在 bootstrap/http 装配。运行和验收见 docs/runbooks/business.md。

职责：学校资料、成员关系、学校角色、绑定策略和学校运营配置。

约束：请求 school_id 不构成授权；所有资源查询必须限定调用者授权范围。

模块边界：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

业务与权限回归位于 backend/tests/business.rs，共享 client 位于 api-contract/src/modules/business。
