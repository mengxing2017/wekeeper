# @wekeeper/api-contract

不部署的 TypeScript DTO/client 包，按 http、system、health、business 子路径导出。
前端仅在 feature/api/index.ts 引用本包，不重复定义 HTTP DTO 或另写 fetch。

## 单一 JSON:API 业务协议

`/api/v1` 结构化成功和错误响应统一为 `application/vnd.api+json`，后端使用固定版本的 Cairnkit `Resource`、`DocumentData`、`PrimaryData`、`error_document`。没有旧 `{data: DTO}`、`{error: ...}` 解包分支。

- 单资源：`{"data":{"type":"groups","id":"…","attributes":{"title":"…"}}}`。
- 集合：`{"data":[资源对象,…]}`；空集合为 `[]`。
- 无返回资源的命令（注销、领取、授权等）：`{"data":null}`。
- 错误：`{"errors":[{"status":"422","code":"validation","title":"Unprocessable Entity","detail":"…"}]}`；HTTP 状态码不折叠成 200。
- 写入：`{"data":{"type":"groups","attributes":{"name":"…"}}}`；更新 DTO 的 `id` 由 client 移到资源顶层。服务端验证资源类型，拒绝扁平 JSON、保留字段冲突及不支持的关系写入。无参数命令不需要请求体。

client 统一编码/解码协议，向 feature 提供 `{id,...attributes}` 领域 DTO；列表返回 `{items,page?}`。只接受 JSON:API 成功文档和媒体类型，不能靠旧格式回退。`ApiError.status/code/message` 保持稳定；网关 HTML 或畸形错误使用通用提示。请求限定同源，保留 Cookie、CSRF、AbortSignal 和 Idempotency-Key。

健康探针 `/healthz`、`/readyz` 使用普通 JSON；图片上传请求为二进制、下载响应为图片；OAuth 是重定向。这些是明确的传输用途，不提供第二套业务 JSON 协议。旧 `/api/discover/*` 业务兼容路由已退出，历史图片 token 读取仍可用。

## 分页与任务记录

分页列表采用 `page[number]`、`page[size]`，默认 1/30，size 为 1–100，number 为 1–100000。旧 `page/page_size`、未知字段、重复分页字段与非整数返回 400；整数越界返回 422。数据库多取一条，响应只保留请求页大小，不伪造 total。普通 collection 使用 `meta.page = {number,size,has_more}`；v2 管理列表保留 singleton 的 `{items,has_more}`，包括广告、广告版本、投放、业务目录、来源凭据、来源路由、凭据关联路由、来源回执和即时消息。前端据 `has_more` 控制翻页。完整选择列表不分页、不暗中截断，故没有 `meta.page`。

`GET /api/v2/admin/messages` 仅允许已登录、账号启用的系统管理员查询，支持 `route_id`、UUID `receipt_id` 与上述分页参数，支持 `channel`、目标 `stage` 与 `attention` 筛选，在分页前生效；不接受 `search/status/mine/kind`。响应仍为 JSON:API `messages/result`，attributes 为 `{items: BusinessMessage[], has_more: boolean}`。行 `id` 为 receipt_id，message_id 在内部交接前为 null；主状态由实时目标结果推导。progress 包含按通道和群统计的总数、诊断、观察时间、版本与汇总滞后标识，milestones 记录接收、交接、通道任务创建事实。`GET /api/v2/admin/messages/{receipt_id}/deliveries` 按相同分页参数返回目标群、当前阶段和原因、设备/发送身份、任务和尝试引用，以及快照版本；仅系统管理员可读。这些管理协议不属于对外来源消息 SDK。


## 验证

```bash
pnpm --dir api-contract build
pnpm --dir api-contract test
```

`fixtures/system.json`、`wecom-status.json`、`not-found.json` 由真实 Rust 序列化/路由与 TS client 共同核对。公开接口修改需同步后端、contract、fixture 与调用方。后端和前端必须同批发布，旧 client 不再兼容。

## 企业通讯录

`directory` client 提供平台目录状态、部门与成员分页筛选、同步任务、账号绑定与解绑。目录和账号绑定仅系统管理员可操作，不依赖学校或工作空间。绑定资源为 `wecom-bindings`，PUT 提交 member_id/revision，DELETE 用带引号的 If-Match 传递绑定版本。
账号更新只接受 display_name/active，不能提交 wecom_userid。该字段在账号读取中仅是目录关联得到的只读显示值。
`fixtures/wecom-directory-member.json` 由 Rust 序列化契约和 TypeScript client 共同验证。

## 企业群发

`broadcast` client 提供任务草稿创建/修改、确认、目标与事件分页、刷新、提醒、停止和核对。资源全局管理，所有操作校验身份、账号启用状态及系统管理员权限，写操作使用 JSON:API `broadcast-tasks`，确认与管理动作带 Idempotency-Key。Task 汇总、成员状态和群级 official_status 分别建模；人工分享及旧历史读取 client 已移除。Rust 序列化测试与 TypeScript 共用 broadcast-task / broadcast-target fixture。

手动群发仅接受显式 `content_source: text` 与 `text_content`，输出使用冻结 `content`；草稿 PATCH 仍需 revision。群发 fixture 均经过 Rust 实际序列化与 TS client 验证。

本包仅导出当前分发业务的协议与 client。分发使用直接文本和冻结快照；linjian 的协议由其独立项目维护。

企业微信和手机分发分别使用 `/api/v1/wecom-distribution` 与 `/api/v1/mobile-execution` 下的全局资源接口，不提供工作空间、成员或学校授权字段及兼容分支。系统管理员维护使用独立 `/api/v1/platform/administrators`（查询、新增），以及 `/{id}`（编辑、删除）和 `/{id}/password`（重设密码）。

手机分发内部管理记录通过 `@wekeeper/api-contract/mobile-execution/dispatch-records` 查询。接入记录独立于 task_id，提供候选评估、事件和设备交付交互的稳定游标分页；当前原因与历史原因分开过滤。执行包生成、服务端写出与设备落盘确认分开表示。此 client 仅供系统管理员后台使用，不属于对外来源消息 SDK。
