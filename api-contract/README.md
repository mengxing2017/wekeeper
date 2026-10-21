# @wekeeper/api-contract

不部署的 TypeScript DTO/client 包，按 http、system、health、business 子路径导出。
前端 feature/api/index.ts 统一重导出对应模块，不重复定义接口或另写 fetch。
请求限定同源，HTTP 错误转为 ApiError；business client 的写请求携带会话 CSRF。

```bash
pnpm --dir api-contract install --frozen-lockfile
pnpm --dir api-contract build
pnpm --dir api-contract test
```

fixtures/system.json 同时由 Rust 的 HTTP 测试和 TypeScript 测试消费。
更改响应时必须同一变更更新 DTO、handler、fixture 与调用方。
## 企业微信分发查询补充

`workspace.getDraft(school, id)` 按 ID 读取学校草稿；`workspace.relatedHistory(school, id, page)` 返回跨草稿相关历史与全量阻断摘要。列表沿用 `workspace.list`，分发列表支持 `page/page_size`（默认 30，最多 100）。

`RelatedHistory.items` 为当前页，`latest_attempt_id` 指向须引用的最新相关尝试，`blocked` 覆盖全部相关历史而非当前页。字段契约由 `fixtures/delivery-history.json`、Rust 序列化测试及 client 测试共同验证。
