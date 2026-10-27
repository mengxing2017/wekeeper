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
后台 `/admin`、成员社区 `/community`、企业微信 `/wecom`。

需要本地调试企业通讯录时，在启动后端的同一终端先 export `WEKEEPER_WECOM_CORP_ID` 和 `WEKEEPER_WECOM_SECRET`；`WEKEEPER_WECOM_AGENT_ID` 可留空。嵌入式启动不会自动读取 `.env`，完整命令及本地文件注入方式见 [本地开发：企业微信凭据注入](../manual/local-development.md#企业微信凭据注入)。嵌入式入口强制本地登录模式，完整凭据也不会开启 OAuth；服务端群发不依赖本地 origin/登录方式，配置凭据并显式设置 `WEKEEPER_WECOM_BROADCAST_ENABLED=true` 后可调试真实任务。

## Docker 与首次管理员

配置 `.env` 中数据库密码、WEKEEPER_PUBLIC_ORIGIN、HTTPS 域名及平台凭据。
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

1. 平台管理员创建学校和运营账号，在“企业通讯录”同步可见部门及成员，再到账号管理选择并核验绑定企业成员。
2. 学校成员中授予 operator 或 admin；同一账号可以有多个明确的运营学校授权。
3. 配置审核模式、主题、轮播、二维码、客服、法律文本、标签和屏蔽词。
4. 用户通过微信登录绑定学校，或使用管理员分配账号，在社区发布内容。
5. 后台审核帖子和评论；编辑正文会回到待审核。
6. 创建文本模板、新建版本、发布版本。占位符：school.name、post.title、post.summary、post.detail_url。
7. 配置学校客户群，并确保群主绑定有效本校运营账号。打开“群发任务”直接填写正文，或选择社区内容及模板，再选择多个群并保存冻结草稿。直接文本无须社区内容，operator 可保存，须由本校 admin 确认。
8. 预览正文（最多 4000 UTF-8 字节）、目标群与群主，人工确认创建企业微信任务。无需打开每个群侧边栏。
9. API 按群主分批单次创建任务；成员在企业微信确认执行。
10. 在任务详情查看官方逐群结果、提醒与停止。unknown 先人工核对，再由 admin 明确创建再次发送草稿。历史记录位于“历史人工分享”。

平台默认资源在账号/学校管理展开区配置；学校有同类启用资源时覆盖平台默认。
全局和学校屏蔽词同时生效。资源链接必须为 HTTPS 或本站相对地址。

## 平台配置

企业微信配置步骤与真机验收矩阵见 [企业微信接入手册](wecom.md)。

- CorpID / Secret：服务端目录、成员核验、OAuth 与群发凭据。群发应用须在客户联系“可调用应用”范围；AgentID 不再是发送前提。
- WEKEEPER_WECOM_BROADCAST_ENABLED 默认 false，完成应用权限与 4.1.10+ 终端验收后显式启用。
- OAuth 回调：`https://域名/api/v1/wecom/oauth/callback`；`/wecom` 仅显示迁移说明。
- 后台普通登录可组织任务，学校授权与执行群主绑定必须有效。确认及领取时重新校验内容、群主和凭据代次。
- token 缓存在 PostgreSQL，不再生成 JS-SDK 签名/票据或加载外部 SDK。群发由服务端凭据和显式开关控制，与 OAuth 门禁独立。
- 官方创建成功只表示任务创建；结果同步、人工核对和再次发送各自保留证据。无有效配置不模拟成功。

官方创建、结果、提醒和停止接口已核对；实际企业权限和真机发送仍须按 [接入手册](wecom.md) 验收。

## 小程序切换

`uniapp/config/domain.js` 指向新 HTTPS 域名；`baseApiUrl` 仍为 `/api/`。
旧 `/api/discover/Common/*` 与 `/api/discover/User/*` 由 Rust 兼容层承接。
保留整数 ID、表单 POST 和 code/msg/data；旧 token 切换后需要重新登录。
上传使用 `/api/discover/Ajax/upload`，客户端 header 字段已修正。
普通接口需要登录并绑定学校；显式配置 `school.config.public_read=true` 才允许无身份的指定学校阅读。
学校选择目录可公开读取，不暴露群映射或管理配置。
旧短信/手机号接口没有迁移旧供应商，返回明确停用错误；使用微信登录和后台密码重置。
小程序真机回归必须覆盖首页分页、详情、评论回复、我的发布、消息、学校绑定、图片和旧链接。
兼容代码存在和 Rust 测试通过不等同于小程序全路径真机验收。

## 迁移预演

迁移工具不读取项目 `.env`、旧 PHP 凭据，也不会自动连接生产库。
导出工具需要单独 Python 环境安装 PyMySQL；MySQL 配置 JSON 权限必须为 0600。
账号至少应有一致性快照 SELECT 权限，不需要写权限。

```bash
python3 tools/migration/export.py --config /secure/mysql-readonly.json --out /secure/snapshot.json --legal-docs /secure/old-public/legal-docs.json
python3 tools/migration/convert.py /secure/snapshot.json --out /secure/batch \
  --states /secure/reviewed-status-map.json --media-root /secure/old-public \
  --media-map /secure/media-map.json --corp-id 企业ID
# 先阅读 report.json / mapping.json / normalized.json / apply.sql。
# 仅对隔离预演数据库执行；psql 使用受控连接参数或 PGSERVICE。
psql -v ON_ERROR_STOP=1 -f /secure/batch/apply.sql
psql -v ON_ERROR_STOP=1 -f tools/migration/verify.sql
```

默认 statusdata 只接受 `1 -> visible`；2/3 等必须核对来源后提供映射。
不要按旧 SQL 文件名批量执行安装、演示数据或修补脚本。
转换失败时不存在 apply.sql，必须处理 rejected 后重新转换；warnings 同样需要逐项确认。
读取 exported snapshot 的 excluded_tables，确认没有遗漏业务扩展表。
校验学校/审核/展示分组数量、引用关系、正文、图片、账号映射、历史分享记录与法律文档。
旧配置中的企业成员身份必须由平台管理员从已同步通讯录中选择并核验绑定，不能以字符串相似或旧管理员名称猜测。

增量追平仍使用完整快照作为一致性依据，比较后只写变化行：

```bash
python3 tools/migration/convert.py /secure/final-snapshot.json \
  --previous /secure/snapshot.json --out /secure/final-batch \
  --states /secure/reviewed-status-map.json --media-root /secure/old-public
psql -v ON_ERROR_STOP=1 -f /secure/final-batch/apply.sql
```

同一批次可重放，原值冲突整批回滚；源端硬删除需人工审查。
媒体源在两次快照间应保持不可变或按内容版本归档，不能覆盖同一路径后假设可重建旧媒体快照。
最后追平前停止旧学校写入，检查旧 Java/Hook/定时任务不会重启，再启用新学校入口。
若新库已有写入，不可直接切回陈旧 MySQL；先停写和对账，选择前滚修复或受控反向迁移。

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
4. 学校群配置默认使用候选目录。学校 admin 需要有效账号绑定且是群主；operator/member 无候选权限。已被其他学校关联的群不显示；版本冲突需刷新重选。平台手填入口仅用于已知历史群，仍必须读取官方详情。停用已有群无需上游成功。

`WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS` 默认 1800 秒，0 为仅手动，非零最小 60。Compose 已透传；`make dev-embedded` 继承同终端的 export，不自动读取 `.env`。内部通讯录与客户群目录共用限速器（初始每 200ms 一次），群目录 worker 串行处理群任务、20 秒续租。超时重启任务从头重扫，不复用旧游标。

终端日志依次为 `wecom asset sync requested`、`started`、`progress`、`succeeded; snapshot committed` 或 `failed; previous snapshot retained`。按 job_id/dataset/generation 关联；只有提交后才记录成功。processed 是已保存的规范化项计数，total 为枚举成员/群主范围量，不能将两者当百分比。任务详情保留安全错误码及失败历史群主引用，不显示 token、Secret、请求 URL 或游标。

- `wecom_40001`：核对 CorpID/Secret 与应用；token 获取错误也会保留官方码。
- `wecom_60020`：核对真实 API 公网出口可信 IP。
- `directory_snapshot_required`：先完整同步当前代次内部通讯录。
- `cursor_loop` / `invalid_response` / `inconsistent_snapshot`：停止发布并保留旧快照；待上游数据稳定后重试。
- `network_or_rate_limit`：网络、45009 或可重试 HTTP 异常经过有限退避仍失败。
- 历史群主不可见：先修复权限；确认确实缩小范围时，在失败任务详情填写依据并确认范围变化，再重新同步群。只接受对应任务明确记录的不可见错误，当前可见成员仍会被重新枚举；确认操作写审计，原历史群及学校映射保留。

页面展示每数据集最近成功时间、采集任务状态与范围缩小提示。快照并非上游瞬时时间点；超过 24 小时未核验的对象不进入候选和当前统计，平台仍可查看诊断历史。群外部联系人仅保留群成员标识，不关联客户资料，不按昵称合并。范围缺失不是解散或删除证明。

Secret 显式切换会在同一事务废止旧资产任务、标记旧资料待核验并创建新代次群同步任务；学校映射不变。若群任务早于内部通讯录完成而失败，可待内部通讯录成功后手动重试。内部成员核验成功不证明客户联系权限，需单独检查客户群任务错误。

未完成 staging 在任务终结时清理；成功规范化 staging 保留每类当前及上次成功版本，其余终态任务记录按 30 天清理。稳定群身份与历史群成员关系不硬删除，业务学校群和发送引用不清理。不采集头像、手机号、群公告、聊天内容、unionid 或上游完整响应。

运行 `make check-web` 与 `devbox run -- make check-backend test-business`。数据库测试只接受隔离的 `WEKEEPER_TEST_DATABASE_URL`；临时 PostgreSQL 自动创建并销毁。测试替身不替代真实企业权限与可见数据验收：需核对不同群主、仅在群内出现的外部成员、权限缩小和 Secret 轮换，记录采集时间和覆盖范围。
