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

浏览器必须使用与 WEKEEPER_PUBLIC_ORIGIN 完全一致的地址，不能混用 localhost 与 127.0.0.1。
嵌入式库每次停止后删除数据；显式设置开发密码时创建 admin，没有硬编码默认密码。
后台 `/admin`、成员社区 `/community`、企业微信 `/wecom`。

需要本地调试企业通讯录时，在启动后端的同一终端先 export `WEKEEPER_WECOM_CORP_ID` 和 `WEKEEPER_WECOM_SECRET`；`WEKEEPER_WECOM_AGENT_ID` 可留空。嵌入式启动不会自动读取 `.env`，完整命令及本地文件注入方式见 [README：注入企业微信凭据](../../README.md#注入企业微信凭据)。嵌入式入口强制本地登录模式，完整凭据也不会开启 OAuth/客户端分享；验收这些能力需使用普通 API/Compose 的 HTTPS 部署。

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
7. 配置 corp_id/chat_id 的客户群，选择已审核内容和已发布模板生成草稿。
8. 从企业微信客户群应用入口打开 H5，OAuth 登录，读取当前群并核对草稿正文。
9. 人工点击分享；服务器创建尝试并领取单次 SDK 调用，页面执行官方客户端 SDK。
10. 在分享记录查看客户端结果。unknown 先人工核对，再由 admin 明确再次分享。

平台默认资源在账号/学校管理展开区配置；学校有同类启用资源时覆盖平台默认。
全局和学校屏蔽词同时生效。资源链接必须为 HTTPS 或本站相对地址。

## 平台配置

企业微信配置步骤与真机验收矩阵见 [企业微信接入手册](wecom.md)。

- WEKEEPER_WECOM_CORP_ID / WEKEEPER_WECOM_SECRET：服务端目录、成员核验与 OAuth 凭据；WEKEEPER_WECOM_AGENT_ID：当前客户端分享所需的同应用 ID。
- WEKEEPER_WX_APP_ID / WX_SECRET：保留小程序对应 AppID，迁移 openid 必须与之匹配。
- OAuth 回调：`https://域名/api/v1/wecom/oauth/callback`。
- JS-SDK 签名仅允许同源 `/wecom` 完整 URL；页面查询参数参与签名，fragment 不参与。
- 企业微信成员需在客户群中，且数据库学校群映射启用；群信息通过官方接口复核。
- SDK 从 wwcdn.weixin.qq.com 加载；Caddy CSP 已允许该源。
- token、企业/应用票据缓存在 PostgreSQL，通过事务 advisory lock 合并多实例刷新。
- 无有效平台配置时接入功能拒绝调用，不使用模拟成功结果。

当前环境没有企业凭据和真机，未实际向客户群发送消息。官方文档页面本次抓取失败，
SDK 适配依据已有官方接入路径与现有项目调用方式；部署前必须验证应用权限、入口、终端版本和 CDN 版本。

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
