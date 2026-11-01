# 手机共享发送运行手册

## 适用范围与边界

本入口使用 `shared_mobile_send/1`、v8 设备协议和 `shared_mobile_execution_scope/2`。它与企业微信官方客户群群发、v7 无发送现场检查分开。已有企业微信业务保留；旧学校授权与 v7 检查通过均不能自动获得共享发送权限。

当前仓库提供执行链、恢复链和工程证据入库工具。仓库中的测试 fixture 是合成契约数据，不是适配发行或真机验收记录。数据库迁移不写入适配器、生产白名单、手机同意或通过记录。

## 部署准备

1. 备份数据库；在隔离环境先执行新增迁移 `202611010112_shared_mobile_send.sql`。应用启动沿用 SQLx 的自动迁移入口；已执行的迁移不得原地修改。
2. 为 API 配置 `WEKEEPER_SHARED_SEND_KEY`，值为密码学随机生成的 32 字节密钥的 64 位十六进制编码。Compose 已透传此变量。不要把密钥写进仓库、验收 JSON、工单或日志。
3. 密钥用于 AES-256-GCM 加密调用许可、恢复材料与结果敏感内容，必须与数据库备份配套保管，并在所有 API 实例和重启之间保持一致。缺失、格式错误或密钥不匹配将阻止相关发送/恢复，不能回退明文存储。
4. 当前实现是单密钥版本，没有在线密钥轮换机制。存在未结算任务或需要解密的历史响应时，禁止直接替换密钥。轮换须另行实施密文重加密和恢复验证；不能删除旧密钥后认为任务已经安全完成。
5. 同步发布 backend、api-contract、Web 和 keeper-otter。手机确认新授权前，旧授权不升级。不要仅更新 Web 后宣称真机可发送。

### 生成和配置共享发送密钥

在部署服务器的终端执行一次：

```sh
openssl rand -hex 32
```

输出是 64 个十六进制字符（32 字节随机密钥）。将输出完整填入 wekeeper 部署目录的 `.env`，不要修改受版本控制的 `env.example` 来保存真实密钥：

```dotenv
WEKEEPER_SHARED_SEND_KEY=<粘贴上述命令生成的完整输出，不含尖括号>
```

若没有 OpenSSL，可使用 Python 3 的密码学安全随机数生成器替代：

```sh
python3 -c 'import secrets; print(secrets.token_hex(32))'
```

两种命令任选其一，不要在每次启动时重新生成。通过当前部署使用的 Compose 命令重新创建 API 容器，使新环境变量生效；仅重启现有容器不会更新其环境变量。直接运行 API 时，通过进程环境或服务管理器配置同名变量，不能假定程序自动读取 `.env`。

所有 API 实例使用同一个值，并将其保存在部署密钥管理或受限备份中。此密钥用于服务端恢复材料的静态加密，不是设备登录令牌，不需要填入手机。不要用 `docker compose config` 等可能展示完整环境配置的输出向他人核对密钥；不要在报告中粘贴真实值。

## 工程适配发行

维护人员对实际应用版本、真实界面和返回路径核对后，组织独立证据目录。发行 manifest 的字段为：

- `name`、`version`、`source_revision`：实际发行名称、版本及源码提交。
- `environment`：真实手机确认的环境对象，须满足 Environment 校验；不得拿合成 fixture 替代。
- `flow`：完整 v8 FlowV2，包含安全的会话页返回搜索页路径、动态目标/正文绑定与实际可验证的观察规则。
- `evidence`：非空数组，每项为相对路径 `path` 和文件 SHA-256 `sha256`；文件必须位于 manifest 同一证据目录内。

在 wekeeper 根目录执行：

```sh
cargo run --locked --manifest-path backend/Cargo.toml --bin shared-send-release -- \
  --manifest /path/to/reviewed-bundle/release.json \
  --output /path/to/reviewed-bundle/release.sql
```

该命令只校验和生成待审 SQL，不连接数据库、不发布、不点击手机。人工审核证据和生成 SQL 后，按环境数据库变更流程执行。普通管理员不需要编写或导入 JSON，也不得自行伪造维护证据。新应用版本或环境摘要变化需要新的适配发行与检查。

## 验收证据和有限放行

使用 `scripts/shared-send-acceptance.py`，manifest 必须有：

- `device_id`、`school_id`、`target_id`、`release_id`、`environment_snapshot_id`、`reviewer_id`：真实存在且互相匹配的 UUID。
- `stage`：`test_send` 或 `production_allowlist`；`expires_at`：明确的未来带时区时间；`preflight_passed`：真实预检查通过。
- `actual_apk`：实际安装制品的 `path` 和 `sha256`。工具校验 ZIP 包含 AndroidManifest.xml 和 classes.dex；这只能证明文件结构，不能证明已安装或发送成功。
- `artifacts`：非空证据文件列表，每项 `path`、`sha256`，只允许证据目录内的实际非空文件。
- 生产阶段另外需要 `soak_hours >= 72`、`soak_passed: true`、`historical_debt_closed: true`，`cases` 包含 S01—S40 的 `passed`，`case_evidence` 为每个用例列出已验证 artifacts 的摘要引用。

```sh
python scripts/shared-send-acceptance.py /path/to/acceptance/manifest.json \
  --output /path/to/acceptance/acceptance.sql
```

工具只生成带事务的 acceptance_run 和 release_scope SQL，不自动执行。审核人必须查看真实操作记录、APK 安装身份和用例证据；哈希校验不能代替现场真实性审查。数据库还会检查验收记录、设备/学校/群/发行/环境/阶段是否一致，生产阶段是否具备完整用例和托管结论。先有限测试群，再生产有限白名单，始终有有效期。不得为了让页面变绿填入虚构通过值。

## 管理员完整操作路径

1. 平台管理员在共享设备能力页面核对在线状态、环境匹配、可用发行和验收范围，开通匹配的共享能力；配置四维频控、每日上限、时区和允许执行时段。
2. 手机持有人在手机授权页查看新共享能力，使用原生确认框单独同意。撤销、停止和重新配置立即关闭本地执行闸门。后台不会代替手机同意。
3. 学校管理员维护本校独立目标群与共享设备绑定，发起 v8 无发送检查；手机执行检查并回传。检查会过期，环境或授权依赖变化也会失效。
4. 学校运营人员进入共享发送任务，选择目标群并录入纯文本，保存草稿、预览逐群设备分配。管理员核对预览后显式确认。修改正文或分配需重新预览确认。
5. 手机后台在资格满足、未暂停且界面安全时领取任务，逐项准备和获得单次许可，执行后上传固定结果。许可持久化之前、额度/风险校验失败或停止后，不得点击发送。
6. `observed_sent` 仅表示观察到本机发送界面证据，不是对方已收到。`unknown` 必须人工核对，不能自动重试。核对为确未发送后才可按新尝试改派；保留原意图、正文和目标身份及审计。

## 停止、失联与恢复

- 停止不会撤销已经发生的物理点击。手机先关闭本地闸门；服务端停止阻止新许可，保留可能已经点击的风险屏障。
- 租约超时不等于物理界面空闲。有限租约、运行 epoch/fence、静止报告与 unknown 风险共同决定是否可派发新工作，历史维护操作也不能绕过新忙碌条件。
- 手机 journal 分离操作、调用许可和结果 outbox。调用前持久化 consume/calling，单次本地 CAS；重启恢复只回传固定结果或核对已有许可，绝不恢复点击。
- 已发放许可的结果通过 invocation 限定的结果凭据结算，不依赖继续有效的普通设备登录令牌；claim 恢复凭据只能查询该 claim 的既有结果，不能签发新调用。
- 严格比较 ACK 的 event/claim/attempt/invocation/generation/fence/摘要后才能清除 outbox。身份不符或未知响应不清除，不盲目重试 UI。
- 紧急处置先手机停止并平台停任务/撤销发行范围，等待设备静止、核对所有风险项。不要删除任务、调用、风险屏障、同意或验收记录来清空界面。新增审计表使用不可变记录和前向迁移，回退需另行设计。

## 已有验证与待完成项

代码验证记录见工作区 report/手机共享发送闭环编码实施与验证记录.md。当前没有提交任何真实生产适配、白名单或真机通过记录。发布前必须完成真实 APK 构建与安装、插件宿主联调、S01—S40、72 小时托管、隔离数据库多连接竞争/故障注入，以及完整 C01—C24 和 R001—R096 的逐项追踪。不得将本机单元测试数量写成这些矩阵已全部通过。
