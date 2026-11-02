# 接口契约、身份与学校授权

适合同时修改后端和客户端的开发者。共享 DTO/client 只定义在 `api-contract/`，业务 JSON:API 与 Rust 序列化使用共享 fixture 核对。

## 一次接口修改的步骤

1. 明确资源类型、字段、状态、错误码及授权范围。
2. 修改 Rust 请求/响应模型、用例和端口实现；HTTP 层不写 SQL。
3. 修改 `api-contract/` 对应模块类型与 client，同步 fixture。
4. 仅在对应 feature 的 `api/index.ts` 引用 contract，由页面使用该入口；不在 shared 重写 DTO 或绕过 client 请求。
5. 同时执行 TypeScript 契约测试与 Rust 序列化契约测试。TS 编译不能证明 Rust wire 格式一致。

## 凭据与边界

| 调用者 | 凭据与检查 |
| --- | --- |
| Web 后台 | Cookie session、Origin、CSRF 及用例权限 |
| 小程序 | 独立 Bearer transport，不使用浏览器 Cookie |
| 手机设备 | 专用设备凭据与代次/会话，不接受浏览器身份冒充 |
| 公开阅读 | 仅学校显式启用的范围，不能默认全站公开 |

平台管理员与学校授权分开检查。模板、群、设备和任务要核对所属学校及当前有效状态，不能只因为前端可见就跳过后端授权。

## 发送接口

创建、确认、领取和结果上报必须保持当前幂等及 fencing 语义。人工确认、单次调用领取、unknown 人工核对不因新增通道而放宽。不接入千寻、Hook 或 Binlog 发送。

设备 v1—v5 的恢复与在途记录保持原协议，不能强行升级旧请求。当前手机流程见[共享手机通用发送](../runbooks/shared-mobile-send.md)，历史解释见[参考资料](../references/index.md)。

## 业务预览、发布及范围办理

资源、模板、官方成员范围和限额使用短期预览及一次确认；摘要绑定操作者、学校、动作与依赖，确认重验真实权限和修订，响应丢失重试复用结果。业务申请与支持答复留在产品内；安装凭据、检查外部权限、交付和发布仍使用开发/运维工具。

共享输入夹具为 `api-contract/fixtures/product-workflows.json`，Rust 的 `http_contract` 与 TS client 测试共同核对 JSON:API 类型和未知字段。

无法监听数据库 socket 的环境可先运行以下离线检查：

```bash
python tools/check_product_workflows_sql.py --postgres-bin /path/to/postgresql/bin
```

脚本使用临时 PostgreSQL 单用户模式执行前向迁移、准备仓储中的字面量 SQL，并核对旧兼容结构已移除、默认回退、确认结果、申请唯一性和事件不可改写。它不读取 `DATABASE_URL`，不替代显式 `WEKEEPER_TEST_DATABASE_URL` 下的 SQLx 事务、并发和权限测试。真实官方接口、部署及小程序发布仍按对应运行手册验收。

当前首次部署不提供人工分享和 v8 历史任务 HTTP 入口、旧草稿转换或人工采样收尾。正式手机发送要求当前业务授权映射和产品交付记录；断线结果恢复、单次许可与审计仍属于当前任务协议。
