# 官方平台适配

状态：预留模块，尚未编译接线或暴露 HTTP 接口。

职责：微信与企业微信的 OAuth、签名、票据与官方 API 适配。

约束：只接官方能力；密钥、票据及认证 URL 不写日志，不接入 Hook。

实施时按以下边界落地，参考已运行的 system 模块：

- domain/：领域对象、约束与状态转换，不依赖 Axum/SQLx。
- north/local/：用例编排与事务边界。
- north/remote/：路由、DTO 与身份提取，不写 SQL。
- south/port/：仓储与外部能力接口。
- south/adapter/postgres/：SQL 与行转换。

先补业务测试与公开 contract，再在 bootstrap/http 装配。
