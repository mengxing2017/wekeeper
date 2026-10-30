# 手机执行接口运行手册

本手册说明 migration `202610240001_mobile_execution.sql` 引入的通用执行约定和旧 v1 外部身份流程。企业微信可见身份、后台规则及 v3 多群执行见 [规则配置与多群顺序执行](configured-mobile-delivery.md)，不按下文 v1 的外部 ID 登记或固定版本审批流程接入。Android 执行端为独立 `keeper-otter` 项目，已编码 demo 及企业微信 v3 通用引擎；真实规则、APK 和真机发送尚未验收，不包含自动登录。联调 client：`@wekeeper/api-contract/mobile-delivery` 和 `@wekeeper/api-contract/device-execution`。

## 上线顺序

1. 保持 `WEKEEPER_MOBILE_EXECUTION_ENABLED=false`，部署新后端并自动应用 migration。不新增部署服务。
2. 平台管理员登记实际发送账号及负责运营人员。负责人员必须是目标学校 admin。一个真实账号只登记一次；不能把换机、昵称变化当成新账号。
3. 学校 admin 登记规范群身份和核验依据。wecom 群必须传现有 wecom_group_id，identity_namespace=CorpID，external_key=ChatID；wechat 群传 null，使用人工核验的稳定内部标识。群名不用于身份合并。
4. 学校 admin 登记群与 sender 绑定；平台管理员登记设备，再绑定 sender。新设备禁用，不能自动发送。
5. 平台管理员创建一次性 enrollment：显示 8 位数字配对码，10 分钟有效。Otter 只需填写 HTTPS 服务器地址和配对码，服务端自动匹配设备 UUID；无需人工输入设备 ID。执行器发送 code/client_instance_id/protocol_version 领取 30 天专用 token，v1/v2/v3 使用各自协议入口。明文只当次返回；随后平台管理员启用设备。绑定、配对、启用依次提升设备 revision，请始终使用最新值。
6. 核对官方群发/旧分享在途与 unknown，补齐全部历史群映射。学校 admin PATCH mobile-delivery-settings：expected_revision 初次为 0，history_reviewed=true，并填写验收过的 client/script/baseline 精确版本及 evidence_ref。仅设备自报版本不能通过白名单。
7. HTTP 模拟及外部执行器独立验收后再设置总开关 true。开关不依赖腾讯发送 Secret。启用学校的新官方创建、提醒和旧分享 prepare 将返回 409 mobile_channel_exclusive。

## HTTP 约定

管理与旧设备流程使用 `/api/v1`，企业微信新设备执行使用 `/api/v3/device-execution`；历史请求保持原版本。Content-Type/Accept 为 `application/vnd.api+json`。普通逐群独立内容草稿的写请求为：

```json
{"data":{"type":"mobile-delivery-tasks","attributes":{"activity_id":"school-notice-20261024","items":[{"group_id":"规范群 UUID","sender_account_id":"真实账号 UUID","source":{"kind":"text","text_content":"该群专属、经审核的正文"}}]}}}
```

所有 POST/PATCH 必须带 1–128 字符 Idempotency-Key。同主体、操作、资源、key 异参返回 409；网络重试保留原 key，后续新业务操作使用新 key。不要把密钥放在 URL。

Web 管理端通过共享 `createIdempotencyKey()` 调用第三方 `nanoid/non-secure` 生成 32 字符幂等键（`A-Za-z0-9_-`），不再生成 UUID，也不依赖浏览器 `crypto`、`randomUUID()` 或 `getRandomValues()`；局域网 HTTP 或没有 Web Crypto 的浏览器仍可登记手机。后端接收 1–128 字符的 key，不要求 UUID 格式。该实现使用伪随机数，仅用于请求去重，不能用于密码、配对码、访问令牌等凭据；实现说明见 [Nano ID 非加密入口](https://github.com/ai/nanoid#non-secure)。网络重试仍保留原 key，后续新操作生成新 key；Android 执行端固定 HTTPS 源的要求不变。

`nanoid` 已声明为 Web 的运行依赖并写入 `frontend/pnpm-lock.yaml`。Dockerfile 的冻结依赖安装及 `pnpm build:web` 会将其打入 `/srv/assets` 中的 JS，不需要在容器运行时安装 Node 或访问 CDN。发布时需重新构建、发布 Web 镜像并重建对应容器；仅重启旧镜像不会更新前端代码。

浏览器管理沿用 Cookie、Origin、X-CSRF-Token。设备仅接受 `Authorization: Bearer wk_device_...`，Cookie/miniapp token 不能替代；设备 token 不能访问后台业务接口。设备 client 固定 HTTPS origin、credentials=omit、redirect=error，不自动重试请求。设备有效凭据每分钟最多 120 请求，超出 429 device_request_limit。配对另受下文的共享尝试预算约束。

### 短码配对

在「手机设备与发送账号」登记并选择设备，点击「生成配对码」。页面显示如 `0012 3456` 的数字并支持复制；Otter 使用数字键盘，接受连续 8 位数字，也接受中间空格或短横线，保留前导零。设备 UUID 保留作内部关联和运维查看，不是用户需要填写的字段。

执行器请求示例（企业微信使用 v3，演示客户端使用 v1）：

```json
{"data":{"type":"device-credentials","attributes":{"code":"00123456","client_instance_id":"客户端自动生成的实例标识","protocol_version":3}}}
```

`POST /api/v3/device-execution/enrollments` 仍带 Idempotency-Key；成功响应的资源 `id` 是服务端匹配的设备 UUID，原生端随凭据保存，token 不进入 WebView。`device_id` 改为可选，供旧执行器兼容和有未决执行时自动固定原设备。错误设备约束不消费配对码，不能将原未决任务迁移到其他设备或服务器。

- 配对码由后端加密随机数生成，采用拒绝采样得到 8 位数字。数据库只保存摘要，并以唯一约束及重试避免撞码；历史码也不复用。前端用于幂等键的 `nanoid/non-secure` 不用于配对码。
- 10 分钟有效，事务内原子消费一次；重新生成立即废止该设备所有旧码。并发兑换只有一次成功，凭据仅首次返回；同一请求的幂等重放只返回无 token 摘要，响应丢失应重新生成配对码。
- 所有协议和 API 副本共享数据库限次：每 10 分钟全站最多 100 次，每个客户端实例最多 5 次，每个配对码最多 5 次。成功、错误、格式不合法的兑换尝试都计数；计数独立提交，失败事务不能回滚计数。实例标识可伪造，因此全站预算同时限制轮换实例和换码猜测。配对入口达到全站预算时，合法配对同样需等待窗口恢复。
- 无效、不存在、已使用、过期或设备不匹配的码统一返回 `401 pairing_code_invalid`，不暴露码属于哪台设备。限次返回 `429 pairing_rate_limited`、`Retry-After: 600`。原有按设备每 10 分钟 20 次预算继续保留。

部署顺序：先部署含 migration `202610300001_short_device_pairing.sql` 的 API，再更新 Web 镜像和 Otter APK。旧的「设备 ID + 64 字符配对码」请求在原有效期内仍可用；更新后的 Otter 只展示短码入口，已配对设备不要求重新配对。请用新生成的 8 位码验证，不要将历史长码截断使用。

列表 `page[number]`/`page[size]` 默认 1/30、最大 100，meta.page.has_more。无任务是 200 data:null，meta 包含 reason/retry_after_seconds/next_eligible_at；unknown 的 next_eligible_at 为 null。

### 管理资源与写入 type

| 路径（省略 /api/v1） | 方法 | JSON:API type |
| --- | --- | --- |
| `/platform/execution-devices[/{id}]` | GET/POST；单项 GET/PATCH | execution-devices |
| `/platform/execution-devices/{id}/enrollments` | POST | device-enrollments |
| `/platform/execution-devices/{id}/credential-revocations` | POST | execution-devices |
| `/platform/execution-devices/{id}/account-bindings` | POST | device-account-bindings |
| `/platform/sending-accounts` | GET/POST | sending-accounts |
| `/platform/sending-accounts/{id}/delivery-policy` | GET/PATCH | sending-accounts |
| `/schools/{s}/delivery-groups` | GET/POST | delivery-groups |
| `/schools/{s}/delivery-groups/{id}/sender-bindings` | POST | delivery-groups |
| `/schools/{s}/delivery-groups/{id}/delivery-policy` | GET/PATCH | delivery-groups |
| `/schools/{s}/mobile-delivery-capabilities` | GET | mobile-delivery-capabilities |
| `/schools/{s}/mobile-delivery-settings` | GET/PATCH | mobile-delivery-settings |
| `/schools/{s}/mobile-delivery-tasks[/{id}]` | GET/POST；单项 GET/PATCH | mobile-delivery-tasks |
| `/schools/{s}/mobile-delivery-tasks/{id}/previews` | POST | mobile-delivery-previews |
| `/schools/{s}/mobile-delivery-tasks/{id}/confirmations`、`/stops` | POST | mobile-delivery-tasks |
| `/schools/{s}/mobile-delivery-tasks/{id}/items`、`/events` | GET | mobile-delivery-items / mobile-delivery-events |
| `/schools/{s}/mobile-delivery-items/{id}/reconciliations` | POST | mobile-delivery-items |
| `/schools/{s}/mobile-delivery-items/{id}/resend-drafts` | POST | mobile-delivery-tasks |

精确输入字段与响应类型见 contract 的 mobile-delivery 模块；不发送 operation 字段。未知字段拒绝。草稿正文支持 text 和 posts（template_version_id、post_ids），每任务最多 100 群、每群 1–4000 UTF-8 字节，不自动改写/拆分。

### 人工确认与执行闭环

1. 创建 draft，取得 revision。POST previews(expected_revision)，核对逐项正文、限制和 preview_digest；preview 不占发送额度。
2. POST confirmations(expected_revision,preview_digest,content_review=true,scope_acceptance=true,substantive_difference_reviewed=true,evidence_ref)。文本要求学校 admin，帖子要求 operator/admin。不可用“摘要不重复”代替实质差异人工审核。
3. 设备 GET session 获取 binding.revision。POST claims(binding_revision) 自动选择一个已确认项，不能自行指定学校/群。
4. POST claims/{id}/preparations，传 fence、snapshot_digest、观察到的规范账号/群 UUID、精确版本、checks_passed。返回 preparation id。prepared 不代表已发送。
5. POST permits(fence,snapshot_digest,preparation_id)，首次成功获得 invocation_id 和 permit_token，60 秒窗口内仅可本地消费一次。执行前 GET claim，必须 may_execute=true；禁止离线发送。
6. POST results，带 invocation_id/permit_token/event_id、身份/正文摘要、evidence_ref、本地状态和点击时间。仅支持 ui_observed_sent/not_observed/safety_blocked；ui_observed_sent 仅在窗口内、身份摘要匹配且 local_state=sent_bubble_observed 时记录 UI 观察，其余为 unknown。不代表送达/已读。`reported_clicked_at` 可为 null，表示没有可信点击时间；null 无论 result 字符串为何均保守 unknown，不退额或恢复旧资格。
7. permit 前 abort 可释放准备；permit 后 abort/超时/失联保持 unknown 和额度。所有结果回报保存自己的原 Idempotency-Key，回报丢失只用原 key 重试。

设备路径均在 `/device-execution` 下：enrollments/credential-rotations（device-credentials），session（device-execution-sessions），heartbeats（device-heartbeats），claims 与单项/renewals（mobile-execution-claims），preparations（mobile-execution-preparations），permits（mobile-send-permits），results/aborts（mobile-delivery-items）。精确 wire 字段见 device-execution contract。心跳建议 30 秒，准备 lease 120 秒；renewals 不延长 permit。

## 异常恢复

- **permit 响应丢失**：原 key 重试不返回 token，返回原 invocation 摘要并转 unknown。GET claim 核对状态，不能申请第二资格。server 不保存明文 secret，也不能补发新 token。
- **配对/轮换响应丢失**：原 key 只返回无 secret 摘要。平台管理员新建配对，撤销旧凭据。不要反复复用消费过的 code。
- **未知结果**：学校 admin 收集证据，原 execute_before 结束后 POST reconciliations。manually_sent 保留历史成功去重；verified_not_sent 释放对应 hold，但不复活旧 item；重新发送调用 resend-drafts 并重新人工确认。
- **撤权/重绑/停止**：阻止新资格，已授予项转 unknown，不能保证撤回远端已点击消息。换机绑定同一个发送账号，自动关闭旧设备准备/资格，历史额度不变。
- **关闭总开关**：只关闭新 claim/permit；保留回报、查询、核对。关闭学校开关还会关闭该校活动 claim。已占用的额度/unknown 不清空。
- **切回官方**：处置手机 unknown/pending；关联运营账号的最近 24 小时手机成功窗口也阻止新官方调用，避免切通道绕过额度。

稳定 409 code：revision_conflict、device_revoked、binding_changed、quota_exhausted、cooldown_active、duplicate_delivery、official_delivery_pending、school_content_conflict、unknown_requires_review、permit_expired、protocol_unsupported、mobile_channel_exclusive。额度等待不是 429；503 为总开关关闭或依赖不可用。资源查询不返回 token_hash、配对码历史、截图或 UI 树。

## 检查与回退

```bash
make check-web
devbox run -- make check-backend
devbox run -- make test-business
```

test-business 创建一次性 PG17，仅向测试进程注入 WEKEEPER_TEST_DATABASE_URL，退出关闭并删除。也可显式提供隔离 WEKEEPER_TEST_DATABASE_URL 执行 make test-integration。不要使用生产 URL。

回退先关闭新资格，保留结果/核对接口和新表；先处理在途再切换通道。不回滚 migration，不把不了解共享账本的旧 binary 恢复为可发送状态。真机行为与无注入验收不属于本次服务端测试结果。

## 工作台界面

手机执行已有前端入口（2026-10-26 补齐）：

- 平台管理 → **手机设备与发送账号**：`/admin?view=execution-devices`。登记设备、生成一次性配对码、启停设备、撤销凭据、登记真实账号、设置账号限额及绑定手机。
- 企业微信 → **手机发送任务**：`/admin?view=mobile-delivery&school=<学校ID>`。查看学校执行开关、版本审批、登记目标群、授权群账号、设置群限额、创建逐群独立内容草稿、生成冻结预览并人工确认、查看执行记录与事件、停止任务和核对 unknown。

手机任务页要求当前学校运营或管理员授权；平台管理员身份不代替学校授权。配置和核对仅学校管理员可操作；直接文本任务须学校管理员确认。服务端总开关默认关闭，页面会显示实际能力及不可用原因，保存草稿不等于执行。配对码只在生成后显示一次，不保存到浏览器存储；手机执行程序仍须单独部署。

旧 v1 身份配置使用已核验的稳定标识及用户/账号/群 ID；可从平台账号管理、手机账号列表和目标群卡片复制。企业微信 v3 使用可见资料核验和发布配置，外部业务/控件 ID 不可用；策略与逐群进度操作见 [多群执行手册](configured-mobile-delivery.md)。社区内容模式填写已发布模板版本 ID 与已审核帖子 ID。服务器负责最终授权、版本、限额、去重和历史通道互斥核验。
