# Rust 后端开发

适合后端开发者。先按[本地开发](../manual/local-development.md)启动隔离环境，再改动业务模块。

## 分层与事务

`backend/` 按业务模块划分 `domain/`、`north/local/`、`north/remote/`、`south/port/` 和 `south/adapter/postgres/`。HTTP 负责解析、认证接入与序列化，不写 SQL；领域规则不依赖 Axum 或 SQLx。跨对象一致性在业务用例与仓储事务边界维护，授权必须覆盖身份、所属业务范围与具体用例。来源自动发送使用显式路由业务授权，不重复办理工作空间成员权限。

新增能力先明确领域状态和失败语义，再扩展端口、PostgreSQL adapter 与 north 用例，最后接 HTTP。参考已有 system 模块及相关业务模块，不向通用 shared 挤入业务规则。

## 广告素材与投放范围

广告素材通过 `ad_placements` 直接绑定全局、平台或平台下的业务单位，不另建投放规则或规则名称。同一素材在同一范围只能绑定一次；同一范围可绑定多个广告。范围绑定使用 `/api/v2/admin/ad-placements` 管理，素材正文与不可变内容版本仍由 `/api/v2/admin/ad-materials` 管理。

接入消息由独立 `message_orchestration` 唯一驱动，按业务单位读取通道接收配置，持久冻结目标、候选和广告决定后，以事务创建通道委托。配置版本、群独占、内容撤回和来源版本替换不参与即时消息执行。结果通过持久队列有界投影到消息及回执；历史快照和审计保留。

## 移动端任务调度

遵守[候选手机调度 ADR](../adr/0008-mobile-candidate-dispatch.md)：业务任务与设备尝试分离；调度器独立运行，在已确认候选范围内自动换机。关闭调用前旧尝试与签发许可共用数据库协调锁，迟到调用不能复活旧身份。原手机恢复只影响自身资源；已获许可的结果不明只隔离对应目标。不得把本地配置、测试手机、验收或原机静止变成平台受理前提。

## 数据库与锁文件

SQLx migration 放在 `backend/migrations/`，版本递增且 forward-only，普通启动自动迁移。不要修改已部署 migration 来修复线上库。生产使用独立 PostgreSQL；嵌入式入口仅用于隔离开发，退出删除数据。

修改 Cargo 依赖或特性后同步生成并提交 `backend/Cargo.lock`，再验证 `--locked` 构建。Docker 使用 `cargo fetch --locked` 和 `cargo build --frozen --release`；不要移除锁定参数掩盖漏更新的依赖。

## 设备 WebSocket

`/api/v1/device-control/channel` 是设备控制通道，经专用设备凭据认证。处理器位于 `distribution/north/remote/device_control.rs`，只承担传输，不在帧处理中执行 SQL 或业务发送。浏览器 Cookie 不能替代设备凭据。

生产 Caddy 的 `/api/*` reverse proxy 自动转发 WebSocket 升级，文档路由不能覆盖它。验收需合法隔离设备的 101 握手与双向帧；401 或普通 GET 200 都不能证明连接成功。日志不打印令牌。

完成改动后执行[测试与验收](testing.md)，接口变更同时更新[契约](contracts-and-auth.md)。

当前业务边界及授权见[全局分发权限](../guide/workspaces.md)。
