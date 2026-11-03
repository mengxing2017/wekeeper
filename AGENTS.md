# WeKeeper

新架构只有两个部署应用：`backend/`（Rust）与 `frontend/`（React Web 与 uni-app Vue 3 小程序工作区）。
`api-contract/` 是不部署的 TypeScript 类型与 client 包。
旧 PHP、根目录 `uniapp/`（Vue 2 迁移参考）、Java 与历史资源保留供迁移核对，不在新 Docker 镜像中构建或运行。

## 边界

- Rust 按业务模块划分 `domain/`、`north/local/`、`north/remote/`、`south/port/`、`south/adapter/postgres/`。
- HTTP 层不写 SQL；业务规则不依赖 Axum、SQLx。
- SQLx migration 放在 `backend/migrations/`，递增且 forward-only；启动时自动迁移。
- `frontend/apps/web/` 为 React Web，`frontend/apps/miniapp/` 为 uni-app Vue 3 + TypeScript 微信小程序；独立构建，禁止相互 import。管理与运营按 `features/admin/`、`features/operations/` 组织；身份、学校上下文、社区、媒体、资源与反馈放在各自 feature，`shared/` 不保存接口 DTO/client。
- 小程序制品发布到微信平台，不新增 Compose 服务；Web 镜像只构建 contract + web，不构建旧 uniapp 或新小程序。平台中立协议/client 不依赖 DOM，平台请求适配保留独立凭据。
- 只在 feature 的 `api/index.ts` 引用 `@wekeeper/api-contract/*`，不得另写相同 DTO 或绕过 client 请求。
- 公开接口修改同步更新后端、contract 与契约测试。TS 编译不能替代 Rust 序列化契约验证。
- 业务接口必须经过身份、学校范围与用例授权；公开阅读仅在指定学校显式启用时开放。
- 不接入千寻、Hook、Binlog 发送；发送流程遵守人工确认、单次调用领取与 unknown 核对语义。

## 研发流程与业务运行隔离

- 严禁将研发、测试验收、发布交付、产品支持审批等研发流程写成项目业务运行的前置条件、排队状态或放行门禁。研发流程留在测试、CI 与开发文档中，不要求业务人员办理。
- 手机发送不得等待产品交付、验收记录或发行审批后才执行。已确认任务使用冻结的实际执行规则；不得因研发发行状态阻断正常业务。
- 必要的执行配置必须在任务预览/确认时校验。配置缺失或损坏应明确报告配置错误，不能先受理为正常排队任务再无限等待研发处理，也不能伪造规则或验收数据。
- 身份与学校授权、手机解锁及权限、人工确认、额度、物理租约、单次许可和 unknown 核对属于实际业务及执行约束，必须保留。

- 不得从 Git 历史中恢复已删除的手机验收放行、交付激活或旧维护流程手册作为新需求；历史 ADR 与旧测试只解释其当时行为，不得覆盖本节的业务隔离约束。当前手机业务操作以 `docs/guide/operator/mobile-execution-tasks.md` 和 `docs/guide/platform-admin/shared-mobile-resources.md` 为准。

## 验证

```bash
make check-web
devbox run -- make check-backend
```

数据库集成测试只接受 `WEKEEPER_TEST_DATABASE_URL`，不得回退生产地址。
本地无 Docker 调试使用 `devbox run -- make dev-embedded`，自动创建临时 PostgreSQL 17。
嵌入式入口忽略外部数据库配置，退出删除数据；`devbox run -- make test-embedded` 验证生命周期。
部署入口为 `compose.yaml`、`compose.dev.yaml` 与 `env.example`，仅包含 web、api、postgres。
业务边界与授权设计见 [社区业务、学校边界与人工分享 ADR](docs/adr/0002-business-migration.md)，运行方法见根 [README.md](README.md) 与 [业务运行手册](docs/runbooks/business.md)。
