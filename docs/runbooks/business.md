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
后台及企业微信分发入口为 `/admin`，仅包含当前分发业务与系统管理。

需要本地调试企业通讯录时，在启动后端的同一终端先 export `WEKEEPER_WECOM_CORP_ID` 和 `WEKEEPER_WECOM_SECRET`。嵌入式启动不会自动读取 `.env`，完整命令及本地文件注入方式见 [本地开发：企业微信凭据注入](../manual/local-development.md#企业微信凭据注入)。嵌入式入口强制本地登录模式，完整凭据也不会开启 OAuth；服务端群发不依赖本地 origin/登录方式，配置凭据后可按预览、人工确认流程调试真实任务。

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

## 企业微信业务闭环

1. 使用已启用的全局系统管理员账号，同步企业通讯录并核验企业成员绑定及发送资格。
2. 核对客户群和群主，确保群主绑定有效的系统管理员账号。
3. 打开“群发任务”，填写正文并选择客户群，点击“预览”保留草稿。
4. 核对完整正文（最多 4000 UTF-8 字节）、全部目标及执行成员，人工确认创建官方任务。
5. API 按群主分批单次创建任务，成员在企业微信确认执行。
6. 在任务详情查看官方逐群结果、提醒与停止；unknown 保留核对记录，再次发送需另建草稿并确认。

## 平台配置

企业微信配置步骤与真机验收矩阵见 [企业微信接入手册](wecom.md)。

- CorpID / Secret：服务端目录、成员核验、OAuth 与群发凭据。群发应用须在客户联系“可调用应用”范围。
- 群发自动使用有效服务端凭据，无需额外部署开关；实际调用仍校验腾讯应用权限、群主分发授权、额度与官方接口状态。
- OAuth 回调：`https://域名/api/v1/wecom/oauth/callback`；认证成功后进入当前后台群发页。
- 后台普通登录可组织任务，分发授权与执行群主绑定必须有效。确认及领取时重新校验内容、群主和凭据代次。
- token 缓存在 PostgreSQL，不再生成 JS-SDK 签名/票据或加载外部 SDK。群发使用服务端凭据，与 OAuth 登录方式独立。
- 官方创建成功只表示任务创建；结果同步、人工核对和再次发送各自保留证据。无有效配置不模拟成功。

官方创建、结果、提醒和停止接口已核对；实际企业权限和真机发送仍须按 [接入手册](wecom.md) 验收。

## 数据库初始化

项目尚未上线，没有历史业务数据。SQLx 启动时按 `backend/migrations/` 中的当前领域基线创建空库，不创建退场业务表，也不导入或转换历史数据。后续结构变更新增递增的前向迁移，已完成迁移由 SQLx 账本校验。

当前分发账号、手机任务与目标、消息快照、执行授权、发送结果和系统审计属于持续保留的业务记录。

## 验证命令

```bash
make check-web
devbox run -- make check-backend
devbox run -- make test-business
devbox run -- make test-live
devbox run -- make test-embedded
```

`test-business` 创建并销毁隔离 PostgreSQL 17，验证当前业务的 HTTP 与数据库闭环。
其他环境可设置 WEKEEPER_TEST_PG_BIN；已有隔离数据库可设置 WEKEEPER_TEST_DATABASE_URL 后运行 make test-integration。
企业微信真机与 Docker 三容器验证需在相应环境执行。

## 客户群目录

1. 在企微管理后台配置自建应用的客户联系调用权限、成员/群主可见范围和 API 出口可信 IP。单套 CorpID/Secret 同时用于通讯录与客户群目录。
2. 先在「企业通讯录」完成当前凭据代次的完整同步，再进入「客户群管理」点击「同步客户群」。仅创建 groups 数据集任务，群主范围来自当前完整内部通讯录和历史群主，不再调用客户联系成员列表或客户列表/详情接口。
3. 「客户群」详情保留内部成员和群内外部联系人标识；「运营成员统计」仅统计群主群数和参与群数。客户、跟进关系、客户联系成员目录及客户数统计已移除，内部企业通讯录与账号绑定保留。
已经关联为分发目标的群不会重复创建。数据变化时刷新重选。

`WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS` 默认 1800 秒，0 为仅手动，非零最小 60。Compose 已透传；`make dev-embedded` 继承同终端的 export，不自动读取 `.env`。内部通讯录与客户群目录共用限速器（初始每 200ms 一次），群目录 worker 串行处理群任务、20 秒续租。超时重启任务从头重扫，不复用旧游标。

终端日志依次为 `wecom asset sync requested`、`started`、`progress`、`succeeded; snapshot committed` 或 `failed; previous snapshot retained`。按 job_id/dataset/generation 关联；只有提交后才记录成功。processed 是已保存的规范化项计数，total 为枚举成员/群主范围量，不能将两者当百分比。任务详情保留安全错误码及失败历史群主引用，不显示 token、Secret、请求 URL 或游标。

- `wecom_40001`：核对 CorpID/Secret 与应用；token 获取错误也会保留官方码。
- `wecom_60020`：核对真实 API 公网出口可信 IP。
- `directory_snapshot_required`：先完整同步当前代次内部通讯录。Web 客户群页面在首次同步（尚无通讯录快照）或重试此错误时，会先发起/复用通讯录任务，成功后再同步客户群；等待超过 5 分钟会提示稍后重试，不取消后台任务。通讯录失败时不继续同步客户群。没有成功群快照时页面显示“尚无成功快照”，已有快照则继续保留。
- `cursor_loop` / `invalid_response` / `inconsistent_snapshot`：停止发布并保留旧快照；待上游数据稳定后重试。
- `network_or_rate_limit`：网络、45009 或可重试 HTTP 异常经过有限退避仍失败。
- 历史群主不可见：先修复权限；确认确实缩小范围时，在失败任务详情填写依据并确认范围变化，再重新同步群。只接受对应任务明确记录的不可见错误，当前可见成员仍会被重新枚举；确认操作写审计，原历史群及分发目标关联保留。

页面展示每数据集最近成功时间、采集任务状态与范围缩小提示。快照并非上游瞬时时间点；超过 24 小时未核验的对象不进入候选和当前统计，平台仍可查看诊断历史。群外部联系人仅保留群成员标识，不关联客户资料，不按昵称合并。范围缺失不是解散或删除证明。

Secret 显式切换会在同一事务废止旧资产任务、标记旧资料待核验并创建新代次群同步任务；分发目标关联不变。若群任务早于内部通讯录完成而失败，可待内部通讯录成功后手动重试。内部成员核验成功不证明客户联系权限，需单独检查客户群任务错误。

未完成 staging 在任务终结时清理；成功规范化 staging 保留每类当前及上次成功版本，其余终态任务记录按 30 天清理。稳定群身份与历史群成员关系不硬删除，业务目标群和发送引用不清理。不采集头像、手机号、群公告、聊天内容、unionid 或上游完整响应。

运行 `make check-web` 与 `devbox run -- make check-backend test-business`。数据库测试只接受隔离的 `WEKEEPER_TEST_DATABASE_URL`；临时 PostgreSQL 自动创建并销毁。测试替身不替代真实企业权限与可见数据验收：需核对不同群主、仅在群内出现的外部成员、权限缩小和 Secret 轮换，记录采集时间和覆盖范围。


## 共享任务与手机托管同步

新任务冻结正文、目标及已授权候选范围，所选手机为首选。平台在交付前选择候选并检查依赖，通过认证 WSS 控制连接直接下发完整签名执行包；手机持久化后直接本地执行，不再申请许可或联网复核依赖。

设备离线、拒绝、明确执行失败，以及交付和执行超时，只约束对应尝试，平台自动选择下一授权候选。unknown 与迟到结果保留原尝试审计，不阻止其他合法候选承接；设备物理锁只约束该设备，不承诺跨设备绝无重复。

当前调度期限、轮次上限及操作方式见[手机任务手册](../guide/operator/mobile-execution-tasks.md)和[候选手机调度 ADR](../adr/0008-mobile-candidate-dispatch.md)。研发测试与发布流程不作为平台创建、预览、确认、受理或交付合法任务的前置条件。

## 真实等待观察与问题记录

任务详情显示系统首次观察到该等待的时间、最近原因变化和下次重试时间。这些观察由独立后台工作器记录，读取详情不触发全库协调、不占用发送资格。默认连续等待 1800 秒后，以任务和原因去重写入现有问题记录；可通过 `WEKEEPER_SHARED_SEND_WAIT_ALERT_SECONDS` 调整阈值（至少 60 秒）。记录失败、分派或结案不参与领取和发送判断，条件恢复后按原确认与预算自动继续。unknown 保留核对记录，候选调度继续遵守原确认范围和业务预算。

## 手机共享发送运行资源与屏幕状态

平台仅保存群名、正文、发送间隔和业务授权，不安装或校验手机 UI 规则。手机自行完成页面适配，接收完整执行包后直接本地执行；离线、锁屏或本地适配失败不阻止平台预览、确认和受理业务任务。设备按本次尝试回报结果，平台在原授权候选范围内调度并记录换机及迟到结果。
