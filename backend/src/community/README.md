# 社区业务

状态：预留模块，尚未编译接线或暴露 HTTP 接口。

职责：帖子、评论、互动、关注与站内通知。

约束：审核状态与展示状态分离；浏览、点赞和计数更新不得触发发送。

实施时按以下边界落地，参考已运行的 system 模块：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

先补业务测试与公开 contract，再在 bootstrap/http 装配。
