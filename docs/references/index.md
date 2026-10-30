# 协议、源码与工程决策

## 当前协议

`api-contract/` 是统一 TypeScript DTO/client 来源，具体资源以该包源码和与 Rust 共用的 fixtures 为准。业务 `/api/v1` 使用 JSON:API：

```json
{"data":{"type":"posts","id":"资源UUID","attributes":{"title":"示例"}}}
```

错误为 `errors[]`，媒体类型 `application/vnd.api+json`；分页使用 `page[number]`、`page[size]` 和 `meta.page.has_more`。健康探针、二进制图片和 OAuth 重定向保留各自传输用途，不提供第二套业务 JSON 协议。

Web 使用 Cookie/Origin/CSRF，小程序和设备使用各自独立 Bearer；不能混用凭据。只有 feature `api/index.ts` 引用 `@wekeeper/api-contract/*`，不在 shared 定义 DTO/client 或绕过 client 请求。变更公开接口同步 Rust、contract、fixture 和两端测试。

源码入口使用仓库相对路径说明，不依赖浏览器访问本机文件：`api-contract/src/`、`backend/`、`frontend/apps/`、`tools/`。具体手机 wire 路径见 [手机执行接口手册](../runbooks/mobile-delivery.md)；v3 发布、顺序控制和修订见 [企业微信规则与多群执行](../runbooks/configured-mobile-delivery.md)。

## ADR 索引

| 文档 | 决策范围 |
| --- | --- |
| [ADR 0001](../adr/0001-foundation.md) | 初始 Rust/React 骨架；初始 envelope/认证说明已被后续实现替代 |
| [ADR 0002](../adr/0002-business-migration.md) | 社区、学校、身份、媒体、官方任务与迁移边界 |
| [ADR 0005](../adr/0005-mobile-delivery.md) | 手机执行、共享账本、单次资格与 unknown |
| [ADR 0006](../adr/0006-frontend-clients.md) | React Web 与 uni-app Vue 3 客户端分离 |

历史 ADR 用于解释阶段决策，不把早期“尚未实现”或协议示意当作当前事实。正式操作以站内当前运行手册和代码为准，验收记录按日期说明适用范围。

## 关联项目

keeper-otter 是独立 Android 执行端，其正式 APK 制作和原生验收文档由该项目 docs 管理。WeKeeper 只在这里维护服务器与管理端的协议/运行说明，不复制另一项目完整手册，也不把文档站作为手机远控入口。
