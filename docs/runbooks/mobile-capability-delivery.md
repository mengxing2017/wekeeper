# 手机群消息产品交付工具

本文仅供产品交付与部署人员使用。系统管理员及学校的操作见[使用管理](../guide/platform-admin/shared-mobile-resources.md)。不通过浏览器会话导入、批准或读取研发材料。


## 制品与验证

部署 `202611020118_business_scheme_delivery_split.sql` 后使用 `mobile-capability` 二进制。交付文件直接使用 `delivery::Delivery` JSON 对象，不需要签名、公钥或额外包装。部署人员通过已有受控命令行和专用数据库身份导入。

交付 JSON 按代码 `delivery::Delivery` 定义，必须包含：

- 名称、版本、经过验证的 FlowV3、测量来源、测量时间、至少三个真实间隔样本；样本最大值不得超过流程冻结的最小逐群间隔。
- 原始 evidence 与实际受支持的手机助手 apks，每项包含 name、content_hex、sha256。工具重算内容摘要，安装包必须具备 Manifest 与 classes.dex 的 ZIP 结构。
- validation_evidence 的 artifact_sha256s 引用本包原始证据，launch_paths 至少包含 search、all_apps，apk_sha256s 覆盖所有声明的安装包摘要。
- 真实 S01—S40、T01—T14 结果、至少 72 小时观察、历史风险已处理、审阅者。没有默认通过值或虚构证据。
- 交付包不再包含 policy；时区、时段和业务额度由平台管理员独立配置。旧文件须移除 policy；旧签名包装不再接受，需要取出其中的产品对象作为新文件。

交付人员须通过已有交付渠道核对产品来源并审阅真实材料；代码与合成测试不替代外部应用真机结论。

## 独立命令

通过现有部署身份管理建立专用、active=false 且 platform_admin=false 的服务账号，记录其 UUID；该账号无法登录浏览器。部署凭据只在受控终端读取，所有引用保留账号审计。

```bash
# 默认只校验包，不连接数据库，不写数据
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- import --package /secure/product-delivery.json
# 设置受控 DATABASE_URL 后导入，输出 release UUID；不切换当前产品
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- import --package /secure/product-delivery.json --actor SERVICE_UUID --apply
# 发布使用当前 activation 修订；不写入或覆盖业务方案
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- activate --release RELEASE_UUID --actor SERVICE_UUID --expected-revision CURRENT_REVISION --apply
# 撤回阻止后续执行，保留已发调用、历史结果和原始材料
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- withdraw --release RELEASE_UUID --actor SERVICE_UUID --apply
```

导入保留真实外部来源，不冒充设备人工采样；发布原子切换产品生效指针，不修改平台业务方案，使用授权在后台自动关联新发行，继续共享原预算。撤回不能回收已发许可或删除 unknown。工具的 activate/withdraw 在不带 --apply 时只校验参数，不声称数据库预检通过。

旧数据保留原发行、计划、调用和恢复链。已有真实生产授权迁入业务视图；测试授权不升级为生产。升级前后协议仍为 v9，不修改原 wire digest 或 Journal。先在隔离环境跑 `WEKEEPER_TEST_DATABASE_URL=... make test-integration`，并完成实际手机交付验证后发布。

## Docker 标准部署

API 镜像同时发行 `/usr/local/bin/mobile-capability`，与 API 版本一致；不需要在运行镜像安装 Cargo，也不新增常驻服务。先启动数据库并由 API 完成迁移，再运行一次性工具。

不需要配置产品交付签名密钥。`WEKEEPER_SHARED_SEND_KEY` 仍用于发送恢复材料加密，须长期保存并备份；它与产品文件导入无关。

以下命令在宿主机预先安全导出 `DATABASE_URL`（指向 Compose 内的 postgres 主机，使用 URL 编码的用户名和密码）；不要将真实密码或包写进 Git。专用服务身份的创建沿用既有受控账号管理，必须 inactive 且非平台管理员。

```sh
# 校验包，不访问数据库；挂载目录须允许镜像 UID 10001 只读访问
# api 使用与当前部署一致的镜像，不覆盖默认常驻服务入口。
docker compose run --rm --no-deps \
  -v /secure/product:/delivery:ro --entrypoint /usr/local/bin/mobile-capability \
  api import --package /delivery/product.json
# 只读查看交付状态和 activation revision；不返回安装包、凭据或研发材料
docker compose run --rm --no-deps -e DATABASE_URL \
  --entrypoint /usr/local/bin/mobile-capability api status --actor SERVICE_UUID
# 导入不激活，输出真实 release UUID
docker compose run --rm --no-deps -e DATABASE_URL \
  -v /secure/product:/delivery:ro --entrypoint /usr/local/bin/mobile-capability \
  api import --package /delivery/product.json --actor SERVICE_UUID --apply
# 使用 status 显示的 activation revision，初次为 0
docker compose run --rm --no-deps -e DATABASE_URL \
  --entrypoint /usr/local/bin/mobile-capability api activate \
  --release RELEASE_UUID --actor SERVICE_UUID --expected-revision CURRENT_REVISION --apply
# 撤回保留全部证据、调用和 unknown
docker compose run --rm --no-deps -e DATABASE_URL \
  --entrypoint /usr/local/bin/mobile-capability api withdraw \
  --release RELEASE_UUID --actor SERVICE_UUID --apply
```

修订冲突时先运行 status 核对当前激活，不盲目增加修订后重试。导入成功后记录 release UUID，激活响应不确定时先查 status，避免重复导入。工具 status 只读连接数据库；其他动作不带 `--apply` 时只校验包或参数，不保证数据库当前可写或可激活。

激活后平台管理员在“手机群消息使用管理 → 配置业务方案”核对业务时段及额度并显式开放发送；产品激活不自动开放业务。已有业务方案不被升级覆盖，已有使用申请会显示是否需要重新核对。禁止直接改库、伪造验收或删除门禁来开通。
