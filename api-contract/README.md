# @wekeeper/api-contract

不部署的 TypeScript DTO/client 包，按 http、system、health 子路径导出。
前端 feature/api/index.ts 统一重导出对应模块，不重复定义接口或另写 fetch。
默认同源 GET，HTTP 错误转为 ApiError，避免吞掉后端错误码。
身份模块未完成，当前不暴露带副作用的通用请求函数。

```bash
pnpm --dir api-contract install --frozen-lockfile
pnpm --dir api-contract build
pnpm --dir api-contract test
```

fixtures/system.json 同时由 Rust 的 HTTP 测试和 TypeScript 测试消费。
更改响应时必须同一变更更新 DTO、handler、fixture 与调用方。
