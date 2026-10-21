# 企业微信人工分享接入与验收

本轮补齐方案 1.1 的代码与自动化验证。真实企业权限、CDN 可达性和客户群真机行为尚未验收。

## 本地预览与页面恢复

直接访问 `/wecom` 可查看配置状态及登录入口，未配置腾讯凭据也会显示页面。
普通浏览器登录后台后，可在 `/wecom` 选择已授权的学校与草稿进行只读预览；
实际调用 SDK 必须同时满足企业微信客户端、企业成员登录及 HTTPS 应用配置。
尚无学校授权、暂无草稿、草稿链接失效、接口不可达分别有独立提示和恢复入口。
OAuth 失败会回到页面显示授权错误，页面不会把失败的回调 JSON 当作业务界面。

Vite 与 TypeScript 均直接引用 `api-contract` 业务源码，避免本地 `file:` 包副本过期造成导出缺失。
修改 Vite 配置后需重启 `pnpm --dir frontend dev`；浏览器保留旧模块时刷新页面。
懒加载/渲染失败会显示重新加载和返回后台入口；错误边界不是成功加载业务页面的替代品。

公开的 `GET /api/v1/wecom/status` 返回 `server_configured`、`client_configured`、`oauth_available`、`share_available` 四个布尔值，
不暴露企业 Secret、Token、应用 ID 或学校业务数据；状态为 true 也不代表腾讯权限已验收。

## 环境配置

本项目仅支持一套企业微信配置，不需要多企业接入；学校、成员和客户群均在该企业内管理，详见 [单企业架构边界](../adr/0002-business-migration.md#企业微信单企业边界)。

在部署的 API 环境配置以下值，Compose 已提供对应注入位置：

| 配置 | 用途 |
| --- | --- |
| `WEKEEPER_PUBLIC_ORIGIN` | 浏览器使用的 HTTPS 域名，协议、主机、端口必须一致 |
| `WEKEEPER_WECOM_CORP_ID` | 服务端必需的企业 ID |
| `WEKEEPER_WECOM_AGENT_ID` | 当前客户端分享必需的正整数 AgentID，目录/OAuth 可留空；与 Secret 对应同一应用 |
| `WEKEEPER_WECOM_SECRET` | 服务端必需的应用 Secret，仅配置到 API |
| `WEKEEPER_INSECURE_LOCAL_AUTH` | 生产 HTTPS 环境保持 false |

### 使用 make dev-embedded 本地启动

此入口继承当前终端已 export 的 `WEKEEPER_WECOM_CORP_ID`、`WEKEEPER_WECOM_SECRET`、`WEKEEPER_WECOM_AGENT_ID`，不会自动读取 `.env`。应在启动后端前注入；只运行前端的终端无需这些变量。

```bash
export WEKEEPER_WECOM_CORP_ID='替换为企业 CorpID'
# 输入应用 Secret 后回车，不回显。
read -rs WEKEEPER_WECOM_SECRET
export WEKEEPER_WECOM_SECRET
export WEKEEPER_WECOM_AGENT_ID='' # 目录/绑定可留空，当前客户端分享才要求有效 AgentID
# 已具备工具链时也可直接执行 make dev-embedded。
devbox run -- make dev-embedded
```

需要从文件注入时，使用 README 中的 [本地凭据文件加载示例](../../README.md#注入企业微信凭据)。修改配置后重启后端才生效，嵌入式临时库同时重置，须重新同步目录并绑定账号。

嵌入式入口强制启用 insecure_local_auth；即使提供完整凭据、设置 HTTPS origin 或将环境变量 `WEKEEPER_INSECURE_LOCAL_AUTH` 设为 false，OAuth/分享仍关闭。此入口用于目录、绑定和预览调试。实际 OAuth 与 SDK 分享验收必须改用普通 API/Compose HTTPS 入口并关闭 insecure_local_auth。

### 官方应用与业务配置

在企业管理后台配置应用可信域名、域名归属验证、应用可见范围及相应客户联系权限。
OAuth 回调地址为 `https://域名/api/v1/wecom/oauth/callback`。
通过客户群聊天工具栏配置应用入口 `https://域名/wecom`；普通浏览器入口无法验证当前群 SDK 能力。
需要直接预选内容时，入口可带 `school=<学校UUID>&draft=<草稿UUID>`。
授权过程仅接受上述 UUID 参数，选择信息保存在一次性 state 对应的服务端记录中，不接受任意返回 URL。

平台管理员同步企业通讯录后，为运营账号选择并核验绑定企业成员，再独立赋予目标学校 operator 或 admin。
配置学校客户群 corp_id/chat_id 与启用状态；确保该成员在目标客户群内且应用具有读取群详情权限。
当前部署只接入一个企业，新增群的企业 ID 必须匹配环境配置。启用群在保存前读取官方群详情核验；未配置凭证或无读取权限时不能保存启用群。
群身份保存后不可更换，编辑只能调整名称和启用状态。撤销已有群不依赖上游接口，便于平台故障期间及时停用。
学校授权支持搜索本地账号姓名/账号并多选成员；企业通讯录在平台“企业通讯录”入口独立同步。
`/api/v1/capabilities` 同样使用上述四个能力字段，仅表示本地配置与部署门禁，不能证明腾讯授权或接口调用成功。

## 首次配置、补填 AgentID 与凭据轮换

- 首次只配置 CorpID/Secret 即可同步目录、核验绑定；正式 HTTPS 且 `WEKEEPER_INSECURE_LOCAL_AUTH=false` 时可 OAuth 登录。AgentID 为空或无效只禁用当前客户端分享。
- HTTP 或 insecure 本地环境仍可使用后台目录与普通账号预览，OAuth 和分享均关闭。
- 后续补填同一应用的 AgentID 并重启 API，无须重置目录或重新绑定。分享仍需企业微信客户端、学校授权与目标群核验。
- 更换 Secret 后，先将 API 实例统一部署到新配置，再用平台管理员的本地账号登录“企业通讯录”，点击“验证并切换服务端凭据”并确认。此操作读取服务器当前配置，不在浏览器输入 Secret；先验证新配置可读取目录，再按页面读取的旧代次原子切换。验证失败保留原状态；页面版本已过期则刷新后重试。
- 切换取消旧代次排队/运行任务，退出旧企业身份会话，将成员标为待核验，并自动创建新代次同步任务；旧快照保留供诊断，账号、绑定、学校角色均保留。旧实例不能继续领取或发布，也不能把迟到成员核验写入新代次。凭据回退也需显式切换；状态查询不会自动切换。
- CorpID 不同拒绝复用目录，本项目不支持借此切换企业。Secret 与 AgentID 是否对应、官方权限是否满足仍须真实企业验收。

平台写接口 `POST /api/v1/platform/wecom-directory/credential-rotations` 使用 JSON:API 类型 `wecom-credential-rotations`，attributes 仅含 `expected_generation`；需要平台管理员、同源请求与 CSRF。目录状态返回 `server_configured`、`credential_generation`、`credential_rotation_required`，不返回凭据指纹。

缓存按 CorpID/Secret 的版本化 SHA-256 作用域隔离，token 与两种 ticket 使用不同键。AgentID 不影响缓存身份。不要把数据库缓存或内部凭据指纹复制到日志或公开状态接口。

## 代码行为

- 分享准备、单次领取、SDK 配置与签名入口都检查 share_available；结果上报、人工核对与历史读取不额外要求 AgentID。
- OAuth state 有效期五分钟，原子消费后不可重放；成功登录删除浏览器 state Cookie。
- 授权完成后回到 `/wecom` 并恢复学校/草稿选择，仍由学校授权控制数据读取。
- SDK 固定加载 `wecom-jssdk-2.4.0.js`；15 秒未加载成功会提示错误，允许人工再次读取群。
- 初始化成功后同一页面只注册一次；每次分享前重新获取当前客户群。
- 企业签名与应用签名均由服务器产生，保留原 URL 查询顺序，去掉 fragment 后签名。
- 准备分享时服务器从官方群详情接口复核企业成员，外部联系人记录不能冒充内部成员。
- 分享前需要人工确认并领取一次调用资格；领取响应丢失时也不调用发送。
- SDK 90 秒没有结果按 unknown 回报；迟到的 Promise 不能触发再次发送。
- 明确的 SDK 取消和失败分别回报 cancelled/client_reported_failure，其他异常均为 unknown。
- 回报失败或页面关闭后不自动重发，prepared 超时由服务端归入 unknown，随后由管理员核对。
- SDK 成功只记录 client_reported_success，不代表平台送达；再次分享保留原尝试及人工核对事实。
- 未领取调用资格的尝试不能提交客户端结果；页面关闭等情况超时归入 unknown 后人工核对。
- 相关历史跨越同群中内容重叠的草稿，任一未核对 unknown 都会阻止再次分享；不同草稿并发准备按目标群串行处理。
- 分享页可按 ID 加载旧草稿，并分页访问草稿、尝试与相关历史。普通预览链接不会建立目标群上下文，仍须从目标客户群聊天工具栏打开应用。

## 真机验收记录要求

在测试企业和指定测试群操作，逐项记录客户端版本、应用、学校、草稿、尝试 ID、时间及群内核对结果。

| 场景 | 预期 |
| --- | --- |
| 带学校和草稿进入，完成 OAuth | 恢复选择；未绑定账号或未授权学校被拒绝 |
| 正确客户群入口 | 读取当前 chatId，正文预览与不可变草稿一致 |
| 切换群后分享 | 不创建发送调用；重新核对群 |
| 非群成员或权限撤销 | 后端拒绝准备/调用领取 |
| 人工点击一次、快速重复点击 | 同一尝试最多调用一次 SDK |
| 用户取消 | cancelled，不自动重试 |
| 调用期间断网或超过 90 秒 | unknown，人工查看目标群 |
| 领取响应丢失 | 不调用 SDK，不自行生成新发送机会 |
| SDK 已执行但回报失败、关闭页面 | 不自动重发；查看原尝试并人工核对 |
| 迟到结果、已有人工结论 | 保留人工核对事实，不覆盖成“已送达” |
| 管理员明确再次分享 | 引用原尝试并填写原因，原记录继续保留 |

## 自动化验证

```bash
make check-web
devbox run -- make check-backend test-business
```

SDK 测试使用测试替身和虚拟时间；数据库测试使用隔离 PostgreSQL。
覆盖加载超时及重试、注册次数、签名 URL、取消/失败/未知、发送超时与迟到结果、当前群变化、领取和回报丢失。
Rust 覆盖签名摘要、内部成员/外部联系人区分、OAuth 返回路径、state 单次消费和到期。
既有业务测试覆盖 unknown 核对、迟到回报及明确再次分享，不能代替真实企业真机测试。

## 排查

- OAuth 拒绝：检查企业、可信域名、应用可见范围、账号 UserID 映射及 state Cookie。
- 签名失败：检查浏览器完整地址、企业/应用票据权限及服务器时间；查询参数必须参与签名。
- SDK 加载失败：检查 CDN 网络与 CSP `script-src`；不要将加载成功解释为发送权限已具备。
- 当前群读取失败：检查聊天工具栏入口及客户端支持情况。
- 后端平台调用失败：服务端记录腾讯 errcode 数字，不记录凭据、OAuth code 或完整响应正文。
- unknown：先在客户群核对，再由管理员给出结论；不要用重复点击判断上次是否发送成功。

参考入口：[官方 JS-SDK](https://developer.work.weixin.qq.com/document/path/90514)、
[分享消息到当前会话](https://developer.work.weixin.qq.com/document/path/100749)。
早期接入阶段未完成 CDN 和真机验收；本次凭据拆分的官方依据见 [实施方案](../../report/23-企业微信服务端凭证与客户端分享配置拆分实施方案.md)，自动化替身不代表真实发送通过。

## 企业通讯录与账号绑定

1. 配置上述唯一企业的自建应用凭据，并在企业管理端配置 API 出口可信 IP。应用需要读取目标部门及其成员的权限；只对个别成员可见不代表能枚举部门目录。
2. 平台管理员进入 `/admin?view=wecom-directory`，点击“立即同步”。页面显示最近完整成功时间、执行状态和脱敏错误码。首次至少需要一个可见部门。
3. 同步成功后，在“账号管理 → 绑定企微成员”按企业姓名、UserID 或部门选择成员，点击“核验并保存绑定”。同名成员通过 UserID/部门区分；一个企业成员只能绑定一个普通账号。
4. 到学校成员页面独立分配 operator/admin。同步和绑定不会创建账号或授予学校角色，本地昵称也不会被企业姓名覆盖。
5. OAuth 登录和发送准备会实时核验成员；成员停用、未激活、退出或不可核验时不允许继续企业身份操作。目录里暂时缺失的成员标为待核验，不自动删除本地账号或学校授权。
6. 更换/解除绑定会清理该账号现有会话。解绑不调用企业微信接口；本地账号密码登录与企业身份独立，离职后若需禁止所有本地登录，还应停用账号并撤销学校角色。

`WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS` 默认 1800，非零须至少 60；设为 0 仅关闭周期同步，手动任务仍由 backend 执行。任务每 5 秒检查一次，使用数据库租约和发布令牌协调多个 API 实例；进程退出后过期任务可重新领取。目录读取共享数据库限速，token 失效受控刷新；重试不适用于发送 SDK。

### 从终端确认同步结果

默认日志级别已包含同步 INFO 日志。若自行配置了 `RUST_LOG=warn`，启动前改为 `RUST_LOG=wekeeper_api=info` 才能看到开始、进度和成功信息。

| 日志 message | 含义 |
| --- | --- |
| `wecom directory worker initialized; interval 0 disables scheduled sync only` | 执行器初始化，查看 server_configured、interval_seconds；不是同步成功。 |
| `wecom directory sync requested; queued or existing active job returned` | 手动同步请求已受理，可能复用已有排队/运行任务；不是同步成功。 |
| `wecom directory sync started` | 已领取任务并开始读取目录。 |
| `wecom directory sync in progress; collecting visible directory` | 长任务每 20 秒输出一次，表示仍在采集且续租成功。 |
| `wecom directory sync succeeded; snapshot committed` | 完整目录已提交数据库，department_count、member_count 为本次同步的部门数及去重成员数，elapsed_ms 为本轮执行耗时。 |
| `wecom directory sync failed; previous snapshot retained` | 采集失败，本次未发布，旧快照保留；查看 error_code。 |

任务执行日志的 JSON `span` / `spans` 中包含 `job_id` 和 `generation`，可按任务 ID 与后台“最近任务”关联；期间官方接口返回的错误码也带有同一任务上下文。续租、发布及失败状态保存出错分别记录 stage 为 lease_renewal、publish、record_failure；发布报错时不会输出成功日志，应到后台核对任务状态。空闲轮询不逐次输出日志。

日志不包含 Secret、token、凭据指纹、成员姓名、UserID 或部门名称。出现开始日志并不代表成功，以 `snapshot committed` 或后台任务 succeeded 为准。

同步期间数据暂存于执行器内存，完整成功后在一个事务中更新目录和快照。失败保留旧目录，已绑定但未出现在部门扫描中的成员会补做单成员核验。保存最近两次成功快照，其他快照正文清空，完成任务记录保留 30 天。进程中断会重新读取本轮，不从内存进度续传。

当前实现采用周期对账，不接收通讯录变更回调。目录变化并非实时；不能承诺离职瞬时撤权。成员不可见、网络错误和真实停用分别处理；报错中的 `wecom_数字` 对照官方错误码排查可信 IP、凭据、可见范围和限流。

本次按项目要求直接修改 `202610200002_business.sql`，没有新增 migration 或保留任意 UserID 写入接口。已执行旧版 migration 的开发数据库需要重新初始化再使用；不要通过修改 `_sqlx_migrations` 校验和伪装完成结构升级。本次编码没有操作既有业务数据库。
