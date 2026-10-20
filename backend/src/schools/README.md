# 学校与授权

状态：预留模块，尚未编译接线或暴露 HTTP 接口。

职责：学校资料、成员关系、学校角色、绑定策略和学校运营配置。

约束：请求 school_id 不构成授权；所有资源查询必须限定调用者授权范围。

实施时按以下边界落地，参考已运行的 system 模块：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

先补业务测试与公开 contract，再在 bootstrap/http 装配。
