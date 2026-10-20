# 模板与人工分享

状态：预留模块，尚未编译接线或暴露 HTTP 接口。

职责：模板版本、正文快照、草稿、发送尝试与人工核对。

约束：SDK 成功不等于送达；unknown 不自动重试，目标必须绑定学校与当前客户群。

实施时按以下边界落地，参考已运行的 system 模块：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

先补业务测试与公开 contract，再在 bootstrap/http 装配。
