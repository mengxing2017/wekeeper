# 来源接入 v2

每个下游对应一条调用方凭据，对外只配置 key_id 和 secret，不再提交 source_id。内部 source_id 仅用于已有密文、回执和幂等记录；机器认证元数据保留该字段，由下游后端自动读取，不展示或要求人工填写。

消息接收管理左侧仅保留“调用方凭据”，其“消息分发路由”是所选凭据的子项，不注册独立导航。新增凭据时在同一事务生成内部调用方身份 source_id；来源与凭据合并维护，不提供独立来源平台管理 API。在凭据子项中创建该调用方的路由并授权给当前 key；归属自动带入，管理接口按 key_id 过滤，后端解析内部归属。单位名称、凭据备注、路由名称及路由备注分别保存。

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

本模块只持久受理，`sending=null`，不调用任何真实群发送接口。路由停用拒绝新消息，有效关联可查历史；key 失效拒绝认证、提交及查询。删除不取消历史任务。

集成测试显式使用隔离 `WEKEEPER_TEST_DATABASE_URL`，并配置上述主密钥：

```bash
cargo test --manifest-path backend/Cargo.toml --test source_access -- --ignored
```

`tests/source_v2.sql` 可在迁移后的隔离数据库运行，验证数据库约束；它不替代两个 API 实例的并发测试。正式部署前需配置反向代理连接/请求限流，代理不得修改签名路径和原文，也不得转发重定向后的请求。
