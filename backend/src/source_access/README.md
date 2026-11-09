# 来源接入 v2

每个下游对应一条调用方凭据，对外只配置 key_id 和 secret，不再提交 source_id。内部 source_id 仅用于已有密文、回执和幂等记录；机器认证元数据保留该字段，由下游后端自动读取，不展示或要求人工填写。

消息接收管理提供“调用方凭据”“业务单位与分组”“路由执行配置”“收件与回执”四个入口。路由创建与接入授权仍是凭据的子项；独立执行配置入口面向系统管理员及获授权的工作空间管理员。新增凭据时在同一事务生成内部调用方身份 source_id；来源与凭据合并维护，不提供独立来源平台管理 API。在凭据子项中创建该调用方的路由并授权给当前 key；归属自动带入，管理接口按 key_id 过滤，后端解析内部归属。单位名称、凭据备注、路由名称及路由备注分别保存。

WeKeeper 是上游，Linjian 是下游调用方。请求均由 Linjian 发起，WeKeeper 不回调、不检查下游在线状态，不保存可信网址或内容路径前缀。detail_url 仅作为消息内容，校验 HTTPS 格式，不抓取。source_callers 仅保留稳定身份和授权修订，以保留幂等身份、历史回执与审计。

持久数据库运行前配置 `WEKEEPER_SOURCE_MASTER_KEY_V1`：32 随机字节的无填充 base64url 编码，作为 AES-256-GCM 主密钥。通过部署 secret 管理，不写入版本库。主密钥必须稳定保留；替换主密钥前须迁移密文，不能直接更改环境变量使旧凭据不可解密。数据库保存密文、版本及与 key_id 绑定的 AAD，普通列表与详情不回显 secret，授权查看接口按需解密并返回同一密钥。`make dev-embedded` 在未设置或为空时自动生成仅供本次临时数据库使用的主密钥，不输出密钥值；已有非空配置保持不变。

凭据创建后，可在业务页面详情点击“查看密钥”，反复查看、复制或下载同一密钥，无需后台命令。POST `/api/v2/admin/source-credentials/{key}/secret-reveals` 使用 `source-credential-secret-reveals` 空属性 JSON:API 文档；沿用会话、CSRF 与凭据管理权限，返回 key_id、secret，并设置 `Cache-Control: no-store`。每次成功查看记录审计，不保存 secret 到日志、审计或前端查询缓存。查看不受 `delivery_pending` 限制，不修改凭据有效期、版本或绑定。

仍支持具有受控主机及数据库访问权限的运维人员执行文件交付：

```bash
cargo run --manifest-path backend/Cargo.toml --bin source-secret-delivery -- \
  --key-id <key_id> --operator <operator-reference> --output <new-private-file>
```

命令使用现有 WEKEEPER_DATABASE_* 配置，创建不可覆盖的 0600 文件并记录一次性交付。将文件内容通过组织认可的密钥交付渠道交给对应 Linjian 管理员，按密钥管理制度删除交付文件。命令重复交付被拒绝；页面查看不受此限制，可再次查看原密钥进行核对。数据库账号及密钥应仅授予有交付职责的人员。

机器路径为 POST `/api/v2/source-authentications`、POST `/api/v2/source-route-checks`、POST `/api/v2/source-events` 和 GET `/api/v2/source-receipts/{receipt_id}`。JSON:API、字节规范、限制及错误目录位于本项目 `api-contract/source-v2/`。认证没有 access_token，消息请求无需先单独认证，但每次都需新 nonce、当前时间和签名。GET 幂等头为原 event_id，不能使用 receipt_id 代替。

凭据列表支持 page、search（标识/名称）、key_id、status、valid_from、valid_until（时间上界）筛选；更新带 revision；route_ids 表示期望的完整关联集合，空数组撤销所有关联。路由关联详情单独分页。不提供轮换接口；签发的凭据与下游绑定。`valid_until: null` 或省略截止时间表示长期有效，指定截止时间时必须晚于生效时间；鉴权仍检查状态及生效时间。删除在同一事务中物理删除凭据、关联路由、路由授权与 nonce；保留历史收件、inbox、任务及审计，历史路由标识不依赖现存路由外键。迁移清理既有软删除记录。

收件与 inbox 同事务持久，事件唯一键为 `(source_id,event_id)`，相同摘要返回原 receipt，不同摘要冲突。nonce 独立提交后不因正文无效回滚。来源锁使鉴权/删除形成确定顺序。每 key 每分钟最多 600 个通过签名的请求，计数与 nonce 在共享数据库内；过期 nonce 分批清理，事件幂等身份不随之删除。

接收成功只表示持久受理。API 内的固定周期消费者按路由冻结配置、内容和目标，幂等创建现有企业微信群发或手机候选调度任务；发送结果从实际通道账本汇总到 `sending`，不会把收件成功标记为已发送。草稿、暂停或配置缺失保持积压，修复后自动重试，无需调用方重发。内容版本由本项目持久投影；撤回停止尚可控制的提交，已提交或已签发事实保留。路由停用拒绝新消息，有效关联可查历史；key 失效拒绝认证、提交及查询。删除不取消既有授权任务。

集成测试显式使用隔离 `WEKEEPER_TEST_DATABASE_URL`，并配置上述主密钥：

```bash
cargo test --manifest-path backend/Cargo.toml --test source_access -- --ignored
```

`tests/source_v2.sql` 可在迁移后的隔离数据库运行，验证数据库约束；它不替代两个 API 实例的并发测试。正式部署前需配置反向代理连接/请求限流，代理不得修改签名路径和原文，也不得转发重定向后的请求。

单位和分组是本项目内部目录，不是学校或第三类工作空间。系统管理员维护目录、凭据与明确的路由管理授权；获授权的分发管理员还须具备所选通道工作空间管理权限。完整修订以 CAS 保存并保留不可变历史。启用/恢复会明确授予来源消息自动执行权限，实际执行身份为 `source-routing`，不会伪造人工逐条确认。新增目标或手机绑定不扩大既有任务快照；暂停、撤权和退役在实际提交或签包前复核，已交付手机包无需联网复核。

## 版本投影、检查与回执

当前发布目录为 `api-contract/source-v2/manifest.json` 的 `source-v2-4`。事件原文摘要保持原始字节语义；版本载荷指纹使用 `source-payload/1` 的固定字段顺序，见共享 fixture。相同序号等价事件关联规范回执，相同序号不同载荷异步拒绝；低序号记录观察水位，首次撤回也建立墓碑。内容更新先使旧版不再取得新执行包或官方提交资格；已有签包、提交、成功或 unknown 时不自动再次群发。原快照和真实结果不修改。

规则按 global/source/unit/route/workspace 的不可变版本合并；workspace 身份包含 mobile/wecom 通道。原文标题、正文独立检查，选定工作空间后加查附加词，最终完整正文（含链接、实际换行）再次检查。`nfkc-fullfold/1` 固定 Unicode 18.0.0（内置官方 CaseFolding，NFKC → full default case folding → NFKC）；不折叠空白、不删除零宽字符，也不改写签名字节或 `source-payload/1`。规则与输入摘要、范围版本向量、回执和组合器共同绑定证据；匹配在事务外执行，落库与签包/官方提交在既有互斥事务内做 CAS，不能使用过期资格。

迁移将旧 `source_check_policy` 导入全局版本并禁止再写；旧检查仅供历史阅读。真正空范围也保存范围头和空版本，缺失范围或存储故障保留重试，不按空集放行。新对象通过数据库触发器初始化范围；删除 live 凭据/路由不抹除历史范围版本。规则发布创建持久批次，每轮扫描最多 50 个受影响当前回执；无广告任务重检冻结正文，广告任务按下述全未执行恢复规则处理。可控目标的临时过滤记录与手动停止分开，放宽规则恢复原任务；成功及 unknown 事实保留。撤回优先于规则恢复处理，检查器故障不阻止墓碑。

管理接口 `keyword-rule-scopes`、`versions`、`publications` 以及回执的 `checks`、`checks/{check}/hits` 对每次访问重新校验权限并记审计。系统管理员维护全部范围，普通工作空间管理员只维护自身附加规则；路由授权不授予上游词集修改权。发布必须携带 expected_revision 和 reason，不接受 outcome/force_pass。每范围最多 2000 词、有效并集最多 10000 词；每词原始 512 字节、规范化后 128 个标量。命中规则完整分页，位置样例最多 100 条，标量左闭右开；重叠计数保留，工作预算超出真实重试。

无广告链路继续使用 `source-text/1` 合成标题、正文、链接并统一通道换行。`check_candidate` 提供最终正文候选排除协议，保留 slot_id/advertisement_version，不消耗游标。本卷已接入 fixed 广告候选与同事务组合持久化；真实序列、独立重复槽位及游标消费归第 08 卷，当前不宣称真实广告序列验收通过；这不影响合法无广告消息处理。已交付手机包仍直接本地执行，没有新增手机联网检查。

管理端点位于 `/api/v2/admin/source-routes/{route}/receipts`，列表每页 30 条，详情返回最近 100 条处理与检查历史。`POST .../{receipt}/rechecks` 的 `source-receipt-rechecks` 属性为 revision、reason；会话及 CSRF 之外，还校验路由和当前工作空间管理授权、独立 can_recheck 授权、当前未撤回版本及处理修订。系统管理员可重检全部路由的合法当前内容。

素材应用与实际发送分别表达。sending 通过结果变更队列每轮最多汇总 50 个回执，以目标去重，优先级为成功、未解 unknown、明确失败或停止、待执行；撤回和等价重复可引用原任务摘要。计算版本为 `targets/1`，内部保存总数、修订和更新时间。迟到结果持续归账，不能把终态视为摘要冻结。

机器 GET 仍验证原来源、现存 key/路由授权与原 event_id。每来源每分钟最多 600 次合法回执查询，另保存分钟级计量；既有每 key 请求预算继续生效。成功响应 `X-Source-Poll-After: 5` 建议正常轮询间隔；429、503 依据 Retry-After 退避。retryable=false 只表示不建议重发原请求，不代表停止查询发送结果。

## 广告共享与组合

`advertising/` 独立管理单位所属广告。内容版本和投放策略分别追加，不修改历史，owner_unit_id 不可改变。初始授权只能是本单位显式选择的通道工作空间；共享采用 unit/group/route 精确范围和通道工作空间，禁投优先。共享接收方须同时拥有对应路由与工作空间管理权限，只有选用权；内容编辑与共享管理由系统管理员或单位广告专项权限分别授权，不依赖学校或第三类工作空间。

路由默认 none，可主动选择 fixed、前置/后置、不可用时无广告回退/等待修复、全未执行自动恢复。来源公共原文与工作空间原文通过后才读取广告。`source-ad-text/1` 使用 `【广告】`、正文和可选 HTTPS 链接，与来源文本用两个 LF 分隔；不截断、不拆分、不抓取网页，最终统一 CRLF/CR 为 LF。企业微信最多 4000 UTF-8 字节；手机最多 2000 Unicode 标量和 8000 字节。最终完整正文经过相同关键字检查。

广告选择、不可变组合、三个检查引用、一个物理正文快照、执行代际和当前指针在同一事务提交；手机所有目标和企业微信任务都引用该快照。回执是稳定业务意图；通道子任务可换代但保留完整历史，查询和迟到结果汇总按逻辑目标去重。仅当所有历史代际都无签包、可能官方提交、成功或 unknown，且原配置允许恢复，才可用同一 fixed 广告的合格版本或已选无广告回退替换。旧子任务停止，新代际继承原时限和调度预算；已开始的任务不改正文。正常内容版本发布只影响新消息。

策略变更、到期及配置变更写持久待办，每轮至多枚举 50 个当前回执，保存游标、错误和退避；即时签包/官方 claim 保护不依赖批次已经扫描。广告存储故障失败关闭，none 不查询无关广告目录，撤回独立优先处理。决策失败也保存不可变原因；预览仅返回候选正文、长度、排除原因和当前关键字检查，不签发准入或创建任务。

管理 API 位于 `advertisements`、`routing-units/{unit}/advertisement-managers` 与 `source-routes/{route}/advertisement-configuration`、`composition-previews`、`receipts/{receipt}/compositions`。发布使用 expected_revision/reason；每次范围访问记系统操作审计。管理列表先按权限筛选再 SQL 分页，每页 30 条。协议及 Rust/TypeScript 共享序列化 fixture 随 `source-v2-4` 发布；来源事件原文和机器接收协议不变。
