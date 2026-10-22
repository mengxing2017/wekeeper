# @wekeeper/api-contract

不部署的 TypeScript DTO/client 包，按 http、system、health、business 子路径导出。
前端仅在 feature/api/index.ts 引用本包，不重复定义 HTTP DTO 或另写 fetch。

## 单一 JSON:API 业务协议

`/api/v1` 结构化成功和错误响应统一为 `application/vnd.api+json`，后端使用固定版本的 Cairnkit `Resource`、`DocumentData`、`PrimaryData`、`error_document`。没有旧 `{data: DTO}`、`{error: ...}` 解包分支。

- 单资源：`{"data":{"type":"posts","id":"…","attributes":{"title":"…"}}}`。
- 集合：`{"data":[资源对象,…]}`；空集合为 `[]`。
- 无返回资源的命令（注销、领取、授权等）：`{"data":null}`。
- 错误：`{"errors":[{"status":"422","code":"validation","title":"Unprocessable Entity","detail":"…"}]}`；HTTP 状态码不折叠成 200。
- 写入：`{"data":{"type":"posts","attributes":{"title":"…","content":"…"}}}`；更新 DTO 的 `id` 由 client 移到资源顶层。服务端验证资源类型，拒绝扁平 JSON、保留字段冲突及不支持的关系写入。无参数命令不需要请求体。

client 统一编码/解码协议，向 feature 提供 `{id,...attributes}` 领域 DTO；列表返回 `{items,page?}`。只接受 JSON:API 成功文档和媒体类型，不能靠旧格式回退。`ApiError.status/code/message` 保持稳定；网关 HTML 或畸形错误使用通用提示。请求限定同源，保留 Cookie、CSRF、AbortSignal 和 Idempotency-Key。

健康探针 `/healthz`、`/readyz` 使用普通 JSON；图片上传请求为二进制、下载响应为图片；OAuth 是重定向。这些是明确的传输用途，不提供第二套业务 JSON 协议。旧 `/api/discover/*` 业务兼容路由已退出，历史图片 token 读取仍可用。

## 分页和发送历史

帖子、评论、审计、分享草稿、分享尝试、相关历史、通知、反馈采用 `page[number]`、`page[size]`，默认 1/30，size 为 1–100，number 为 1–100000。旧 `page/page_size` 被拒绝。数据库多取一条，响应只保留请求页大小，并提供 `meta.page = {number,size,has_more}`；不伪造 total。前端据 `has_more` 控制翻页。学校/账号/成员/配置等完整选择列表不分页、不暗中截断，故没有 `meta.page`。

`workspace.getDraft(school,id)` 直接读取历史草稿。`workspace.relatedHistory(school,id,page)` 的 wire `data` 是 `delivery-attempts` 集合，`meta.latest_attempt_id`、`meta.blocked` 保留全量历史阻断语义；client 返回 `{items,page,latest_attempt_id,blocked}`。

## 验证

```bash
pnpm --dir api-contract build
pnpm --dir api-contract test
```

`fixtures/system.json`、`wecom-status.json`、`post-created.json`、`delivery-history.json`、`not-found.json` 由真实 Rust 序列化/路由与 TS client 共同核对。公开接口修改需同步后端、contract、fixture 与调用方。后端和前端必须同批发布，旧 client 不再兼容。

## 企业通讯录

`directory` client 提供平台目录状态、部门与成员分页筛选、同步任务、账号绑定与解绑。全目录仅平台管理员可访问；学校内绑定摘要遵循学校权限。绑定资源为 `wecom-bindings`，PUT 提交 member_id/revision，DELETE 用带引号的 If-Match 传递绑定版本。
账号更新只接受 display_name/active，不能提交 wecom_userid。该字段在账号读取中仅是目录关联得到的只读显示值。
`fixtures/wecom-directory-member.json` 由 Rust 序列化契约和 TypeScript client 共同验证。

## 企业群发

`broadcast` client 提供任务草稿创建/修改、确认、目标与事件分页、刷新、提醒、停止和核对。所有操作绑定学校，写操作使用 JSON:API `broadcast-tasks`，确认与管理动作带 Idempotency-Key。Task 汇总、成员状态和群级 official_status 分别建模；旧 SDK client 已移除，历史读取仍保留。Rust 序列化测试与 TypeScript 共用 broadcast-task / broadcast-target fixture。
