# 身份与会话

状态：预留模块，尚未编译接线或暴露 HTTP 接口。

职责：管理员、社区用户、会话、平台身份交换及认证策略。

约束：学校权限由 schools 配合；业务写入接入前必须完成会话、CSRF 与 Origin 校验。

实施时按以下边界落地，参考已运行的 system 模块：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

先补业务测试与公开 contract，再在 bootstrap/http 装配。
