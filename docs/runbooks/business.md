# 业务运行、迁移与验收

## 本地启动

```bash
make install
pnpm --dir api-contract build
# 密码至少 12 个字符，不要写入版本库。
read -rs WEKEEPER_DEV_ADMIN_PASSWORD
export WEKEEPER_DEV_ADMIN_PASSWORD
export WEKEEPER_INSECURE_LOCAL_AUTH=true
export WEKEEPER_PUBLIC_ORIGIN=http://localhost:5173
devbox run -- make dev-embedded
# 另一终端
pnpm --dir frontend dev
```

浏览器地址应匹配 WEKEEPER_PUBLIC_ORIGIN；本地 insecure 模式允许同协议同端口的 localhost、127.0.0.1 和 IPv6 loopback，局域网地址或其他端口需显式配置对应 origin。
嵌入式库每次停止后删除数据；显式设置开发密码时创建 admin，没有硬编码默认密码。
后台及企业微信分发入口为 `/admin`。论坛、小程序和审核业务在独立 linjian 运行，本项目的 `/community` 已退场。

需要本地调试企业通讯录时，在启动后端的同一终端先 export `WEKEEPER_WECOM_CORP_ID` 和 `WEKEEPER_WECOM_SECRET`；`WEKEEPER_WECOM_AGENT_ID` 可留空。嵌入式启动不会自动读取 `.env`，完整命令及本地文件注入方式见 [本地开发：企业微信凭据注入](../manual/local-development.md#企业微信凭据注入)。嵌入式入口强制本地登录模式，完整凭据也不会开启 OAuth；服务端群发不依赖本地 origin/登录方式，配置凭据后可按预览、人工确认流程调试真实任务。

## Docker 与首次管理员

配置 `.env` 中数据库密码、WEKEEPER_PUBLIC_ORIGIN、HTTPS 域名及平台凭据。使用调用方凭据功能时，还需生成并设置 `WEKEEPER_SOURCE_MASTER_KEY_V1`，仅注入 API；Docker 不自动生成。生成、配置更新及备份要求见 [Docker 调用方凭据主密钥](docker.md#调用方凭据主密钥-首次配置与已有部署更新)。
本地 HTTP 验证才设置 WEKEEPER_INSECURE_LOCAL_AUTH=true；生产保持 false。

```bash
docker compose --env-file .env -f compose.yaml -f compose.dev.yaml up --build --wait
# 独立命令从 stdin 读取密码，建完即退出，不开第二个 HTTP 服务。
read -rs WK_ADMIN_PASSWORD
printf '%s' "$WK_ADMIN_PASSWORD" | docker compose exec -T api wekeeper-api --create-admin admin
unset WK_ADMIN_PASSWORD
```

生产使用 compose.yaml 和固定 digest 镜像。官方凭据仅注入 api，不进入前端构建变量。
生产环境不设置 WEKEEPER_DEV_ADMIN_PASSWORD。
图片位于 PostgreSQL 持久卷，数据库备份须包含 media_assets。

## 单校业务闭环

1. 系统管理员创建企业微信分发工作空间，为已有分发账号授予分发角色；在“企业通讯录”同步可见成员，再到“发送账号”核验企业成员绑定及使用范围。
2. 企业微信工作空间中授予 operator 或 admin；移动工作空间权限另行维护。
3. 论坛发帖、评论审核、举报反馈及展示配置在独立 linjian 完成。
4. 如需使用历史论坛素材，设置 `WEKEEPER_FORUM_ORIGIN` 为 linjian 的实际 HTTPS origin。素材选择由本服务的分发投影提供，新论坛来源事件接入需单独实施。
5. 核对工作空间客户群和发送范围。
6. 创建文本模板、新建版本、发布版本。占位符：school.name、post.title、post.summary、post.detail_url。
7. 配置工作空间客户群，并确保群主绑定有效当前工作空间运营账号。打开“群发任务”直接填写正文，或选择来源素材及模板，再选择多个群并保存冻结草稿。直接文本无须社区内容，operator 可保存，须由当前工作空间 admin 确认。
8. 预览正文（最多 4000 UTF-8 字节）、目标群与群主，人工确认创建企业微信任务。无需打开每个群侧边栏。
9. API 按群主分批单次创建任务；成员在企业微信确认执行。
10. 在任务详情查看官方逐群结果、提醒与停止。unknown 先人工核对，再由 admin 明确创建再次发送草稿。当前任务的操作记录在任务详情查看。

## 平台配置

企业微信配置步骤与真机验收矩阵见 [企业微信接入手册](wecom.md)。

- CorpID / Secret：服务端目录、成员核验、OAuth 与群发凭据。群发应用须在客户联系“可调用应用”范围；AgentID 不再是发送前提。
- 群发自动使用有效服务端凭据，无需额外部署开关；实际调用仍校验腾讯应用权限、群主分发授权、额度与官方接口状态。
- OAuth 回调：`https://域名/api/v1/wecom/oauth/callback`；认证成功后进入当前后台群发页。
- 后台普通登录可组织任务，分发授权与执行群主绑定必须有效。确认及领取时重新校验内容、群主和凭据代次。
- token 缓存在 PostgreSQL，不再生成 JS-SDK 签名/票据或加载外部 SDK。群发使用服务端凭据，与 OAuth 登录方式独立。
- 官方创建成功只表示任务创建；结果同步、人工核对和再次发送各自保留证据。无有效配置不模拟成功。

官方创建、结果、提醒和停止接口已核对；实际企业权限和真机发送仍须按 [接入手册](wecom.md) 验收。

## 论坛与小程序

论坛和整个小程序由 linjian 独立运行。本项目已删除论坛管理五项功能、小程序源码、专用登录接口及平台凭据配置；旧社区与资源接口不再对外提供。迁移与发布步骤以 linjian 项目文档为准。

## 数据表退场与历史工具

论坛、学校、旧资源发布和无运行消费者的旧发送表已通过前向迁移 `202611080140_retire_unused_tables.sql` 下线。新库按完整迁移链初始化，已运行数据库在升级 API 时由 SQLx 执行新增迁移；已有迁移文件及账本不应修改或清空。

该迁移会永久删除 27 张废弃表的数据。升级前完成数据库备份，并确认论坛数据已迁至 linjian；迁移文件逐项列出删除对象，遇到未预期依赖时事务失败，不使用 `CASCADE` 删除额外对象。当前分发账号、两类工作空间、手机任务与目标、消息快照和发送结果、系统审计继续保留。旧账号绑定学校字段、旧论坛投影触发器及旧公开数字 ID 序列一并移除。

`tools/migration/` 是旧论坛 schema 的只读导出与离线转换参考，不适用于当前 WeKeeper schema。不要把旧工具生成的 `apply.sql` 或旧版本备份直接导入已升级的业务库；转换输出会检查旧表是否存在。论坛数据迁移以 linjian 的当前接口和文档为准。

## 验证命令

```bash
make check-web
devbox run -- make check-backend
devbox run -- make test-business
devbox run -- make test-live
make test-migration
devbox run -- make test-embedded
```

`test-business` 创建隔离 PostgreSQL 17 并销毁，执行 HTTP/数据库闭环、实际 SQL 导入、幂等重放、增量与写冲突回滚。
其他环境可设置 WEKEEPER_TEST_PG_BIN；已有隔离数据库可设置 WEKEEPER_TEST_DATABASE_URL 后运行 make test-integration。
企业微信真机、旧小程序真机、真实 MySQL 快照和 Docker 三容器验收需在相应环境执行。

## 客户群目录

1. 在企微管理后台配置自建应用的客户联系调用权限、成员/群主可见范围和 API 出口可信 IP。单套 CorpID/Secret 同时用于通讯录与客户群目录，AgentID 留空不阻止读取。
2. 先在「企业通讯录」完成当前凭据代次的完整同步，再进入「客户群管理」点击「同步客户群」。仅创建 groups 数据集任务，群主范围来自当前完整内部通讯录和历史群主，不再调用客户联系成员列表或客户列表/详情接口。
3. 「客户群」详情保留内部成员和群内外部联系人标识；「运营成员统计」仅统计群主群数和参与群数。客户、跟进关系、客户联系成员目录及客户数统计已移除，内部企业通讯录与账号绑定保留。
4. 工作空间群配置默认使用候选目录。工作空间 admin 需要有效账号绑定且是群主；operator/member 无候选权限。已被其他工作空间关联的群不显示；版本冲突需刷新重选。平台手填入口仅用于已知历史群，仍必须读取官方详情。停用已有群无需上游成功。

`WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS` 默认 1800 秒，0 为仅手动，非零最小 60。Compose 已透传；`make dev-embedded` 继承同终端的 export，不自动读取 `.env`。内部通讯录与客户群目录共用限速器（初始每 200ms 一次），群目录 worker 串行处理群任务、20 秒续租。超时重启任务从头重扫，不复用旧游标。

终端日志依次为 `wecom asset sync requested`、`started`、`progress`、`succeeded; snapshot committed` 或 `failed; previous snapshot retained`。按 job_id/dataset/generation 关联；只有提交后才记录成功。processed 是已保存的规范化项计数，total 为枚举成员/群主范围量，不能将两者当百分比。任务详情保留安全错误码及失败历史群主引用，不显示 token、Secret、请求 URL 或游标。

- `wecom_40001`：核对 CorpID/Secret 与应用；token 获取错误也会保留官方码。
- `wecom_60020`：核对真实 API 公网出口可信 IP。
- `directory_snapshot_required`：先完整同步当前代次内部通讯录。Web 客户群页面在首次同步（尚无通讯录快照）或重试此错误时，会先发起/复用通讯录任务，成功后再同步客户群；等待超过 5 分钟会提示稍后重试，不取消后台任务。通讯录失败时不继续同步客户群。没有成功群快照时页面显示“尚无成功快照”，已有快照则继续保留。
- `cursor_loop` / `invalid_response` / `inconsistent_snapshot`：停止发布并保留旧快照；待上游数据稳定后重试。
- `network_or_rate_limit`：网络、45009 或可重试 HTTP 异常经过有限退避仍失败。
- 历史群主不可见：先修复权限；确认确实缩小范围时，在失败任务详情填写依据并确认范围变化，再重新同步群。只接受对应任务明确记录的不可见错误，当前可见成员仍会被重新枚举；确认操作写审计，原历史群及学校映射保留。

页面展示每数据集最近成功时间、采集任务状态与范围缩小提示。快照并非上游瞬时时间点；超过 24 小时未核验的对象不进入候选和当前统计，平台仍可查看诊断历史。群外部联系人仅保留群成员标识，不关联客户资料，不按昵称合并。范围缺失不是解散或删除证明。

Secret 显式切换会在同一事务废止旧资产任务、标记旧资料待核验并创建新代次群同步任务；学校映射不变。若群任务早于内部通讯录完成而失败，可待内部通讯录成功后手动重试。内部成员核验成功不证明客户联系权限，需单独检查客户群任务错误。

未完成 staging 在任务终结时清理；成功规范化 staging 保留每类当前及上次成功版本，其余终态任务记录按 30 天清理。稳定群身份与历史群成员关系不硬删除，业务学校群和发送引用不清理。不采集头像、手机号、群公告、聊天内容、unionid 或上游完整响应。

运行 `make check-web` 与 `devbox run -- make check-backend test-business`。数据库测试只接受隔离的 `WEKEEPER_TEST_DATABASE_URL`；临时 PostgreSQL 自动创建并销毁。测试替身不替代真实企业权限与可见数据验收：需核对不同群主、仅在群内出现的外部成员、权限缩小和 Secret 轮换，记录采集时间和覆盖范围。


## 共享任务与手机托管同步

本轮需一起交付后端、Web 与 keeper-otter。后端启动自动应用 `202611030121_shared_send_execution_status.sql`；不运行历史数据回填或旧协议转换。手机通过独立的 v9 `send-delegation-context` 读取托管事实，任务详情每 5 秒读取一次实时执行条件，读取不运行全库协调或预占额度。

使用范围及启用的群绑定就是持久协调意图，服务端工作器定期处理，各使用范围、设备事务分别提交，单个绑定失败通过保存点回滚并在后续周期重试。授权事实、设备连接与本地准备、调度结果及投递风险分层判断。诊断失败不应成为新任务或授权读取的前置。

部署验收需运行显式隔离数据库测试，使用协议模拟端验证离线换机、调用前超时、迟到许可拒绝及单次发送；手机本地执行另行验证，不能作为平台受理条件；详情断网后应显示旧快照而不是重新提交。


## 共享任务等待规则与原领取恢复

新任务冻结正文、目标和已授权候选范围，首选手机暂不可用时自动选择其他合法候选。平台不校验手机 UI 配置，也不等待原手机在线或静止才使用另一台手机。调用前失联关闭旧尝试并禁止其迟到获许可，保留旧设备物理锁和历史记录。

已发放许可且结果不明时仅对应目标待核对；其他目标可继续调度。设备结算只释放原 holder/fence 的物理资源，不成为其他设备推进业务的前提。历史任务仍保留原顺序及单设备授权，需要使用候选池时复制为新草稿核对确认。详见[候选手机调度 ADR](../adr/0008-mobile-candidate-dispatch.md)。


## 真实等待观察与问题记录

任务详情显示系统首次观察到该等待的时间、最近原因变化和下次重试时间。这些观察由独立后台工作器记录，读取详情不触发全库协调、不占用发送资格。默认连续等待 1800 秒后，以任务和原因去重写入现有问题记录；可通过 `WEKEEPER_SHARED_SEND_WAIT_ALERT_SECONDS` 调整阈值（至少 60 秒）。记录失败、分派或结案不参与领取和发送判断，条件恢复后按原确认与预算自动继续。unknown 仍需核对，不自动重发，也不延长人工确认期限。

## 手机共享发送运行资源与屏幕状态

平台仅保存群名、正文、发送间隔和业务授权，不安装或校验手机 UI 规则。手机自行准备页面并申请单次发送许可；离线、锁屏或本地适配失败不阻止平台预览、确认业务任务。手机条件只影响实际执行，结果不确定仍须核对，禁止自动重发。
