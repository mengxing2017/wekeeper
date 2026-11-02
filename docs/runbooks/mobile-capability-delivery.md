# 手机群消息产品交付工具

本文仅供产品交付与部署人员使用。系统管理员及学校的操作见[使用管理](../guide/platform-admin/shared-mobile-resources.md)。不通过浏览器会话导入、批准或读取研发材料。

## 制品与验证

部署 `202611020116_mobile_usage.sql` 后使用 `mobile-capability` 二进制。签名包为 `{ "payload": { ... }, "signature_hex": "..." }`。使用可信产品交付密钥的 Ed25519 签名；签名对象是 payload 解析为 serde_json::Value 后的紧凑 UTF-8 序列化（对象键排序）。部署环境只配置 `WEKEEPER_CAPABILITY_PUBLIC_KEY` 的 32 字节公钥十六进制，私钥不进服务端或 Web。

payload 按代码 `delivery::Delivery` 定义，必须包含：

- 名称、版本、经过验证的 FlowV3、测量来源、测量时间、至少三个真实间隔样本；样本最大值不得超过流程冻结的最小逐群间隔。
- 原始 evidence 与实际受支持的手机助手 apks，每项包含 name、content_hex、sha256。工具重算内容摘要，安装包必须具备 Manifest 与 classes.dex 的 ZIP 结构。
- validation_evidence 的 artifact_sha256s 引用本包原始证据，launch_paths 至少包含 search、all_apps，apk_sha256s 覆盖所有声明的安装包摘要。
- 真实 S01—S40、T01—T14 结果、至少 72 小时观察、历史风险已处理、审阅者。没有默认通过值或虚构证据。
- 完整生产业务政策（含 expected_revision、时区、允许时段、四维额度、间隔及确认/检查期限），mode 只能为 enabled，allowed_stages 只能为 production_allowlist。

签名只验证来源与完整性，交付人员必须审阅真实材料；代码与合成测试不替代外部应用真机结论。

## 独立命令

通过现有部署身份管理建立专用、active=false 且 platform_admin=false 的服务账号，记录其 UUID；该账号无法登录浏览器。部署凭据只在受控终端读取，所有引用保留账号审计。

```bash
# 默认只校验包，不连接数据库，不写数据
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- import --package /secure/product-delivery.json
# 设置公钥、受控 DATABASE_URL 后导入，输出 release UUID；不切换当前产品
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- import --package /secure/product-delivery.json --actor SERVICE_UUID --apply
# 发布使用当前 activation 修订；签名政策的 expected_revision 也必须匹配
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- activate --release RELEASE_UUID --actor SERVICE_UUID --expected-revision CURRENT_REVISION --apply
# 撤回阻止后续执行，保留已发调用、历史结果和原始材料
cargo run --manifest-path backend/Cargo.toml --bin mobile-capability -- withdraw --release RELEASE_UUID --actor SERVICE_UUID --apply
```

导入保留真实外部来源，不冒充设备人工采样；发布原子切换生效指针与政策，使用授权在后台自动关联新发行，继续共享原预算。撤回不能回收已发许可或删除 unknown。工具的 activate/withdraw 在不带 --apply 时只校验参数，不声称数据库预检通过。

旧数据保留原发行、计划、调用和恢复链。已有真实生产授权迁入业务视图；测试授权不升级为生产。升级前后协议仍为 v9，不修改原 wire digest 或 Journal。先在隔离环境跑 `WEKEEPER_TEST_DATABASE_URL=... make test-integration`，并完成实际手机交付验证后发布。
