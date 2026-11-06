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
- 不接入千寻、Hook、Binlog 发送；发送流程遵守人工确认、随任务交付的单次调用授权与 unknown 核对语义。

## 研发流程与业务运行隔离

- 严禁将研发、测试验收、发布交付、产品支持审批等研发流程写成项目业务运行的前置条件、排队状态或放行门禁。研发流程留在测试、CI 与开发文档中，不要求业务人员办理。
- 手机发送不得等待产品交付、验收记录或发行审批后才执行。已确认任务冻结业务意图与授权，手机自行完成本地执行适配；不得因研发发行状态阻断正常业务。
- 业务只填写一个群名称，不重复要求完整群名和搜索词，不提供平台别名；`exact_name`、别名及关联身份版本已删除，不保留兼容分支。群身份缺失、识别与执行契约解释交由设备处理，不得阻塞平台创建、预览、确认、受理、调度或交付；设备失败按该次尝试回报。
- 平台只下发群名、正文、授权及业务调度约束，不管理、安装、校验或下发手机 UI flow。手机端适配与配置缺失不得阻止平台创建、预览、确认和受理任务；具体执行失败由设备按该次尝试回报。平台不得要求测试手机、规则包、人工采样或下游配置作为业务前置条件。
- 设备通过 `task-deliveries` 一次接收完整签名执行包，候选选择与依赖检查均在平台交付前完成。接收落盘后直接本地执行，不再申请执行许可、联网复核依赖或续租；历史申请恢复、显示收件和结果补报不得排在已交付任务前。
- 平台任务、目标投递与设备尝试必须分离。离线、锁屏、权限、本地适配、忙碌、托管准备、租约或恢复状态只约束对应设备或尝试，不得阻塞平台受理和管理合法任务，也不得使任务永久等待某台手机。
- 新任务确认业务内容、目标与授权候选范围，所选手机为首选。在此范围内根据离线、拒绝、失败、超时与反馈自动换机并记录审计，不重复办理业务确认。原手机恢复、静止证明或物理锁释放不得成为另一台合法手机承接确定未发送目标的前提。历史任务不自动扩大授权范围。
- 当前手机离线或明确执行失败时直接切换下一授权候选；接收后 5 分钟没有最终执行结果时切换下一台。平台按固定周期扫描持久结果，不为每个任务驻留线程等待。unknown 与历史许可风险保留审计但不阻止换机；迟到结果按原尝试入账，不覆盖后继身份。离线改派可能产生跨手机重复消息，不承诺同目标跨设备绝不重复。设备物理锁仍约束该设备；身份、学校授权、人工确认、业务额度和单次许可继续生效。
- 当前用户业务要求优先。现行架构规则见 `docs/adr/0008-mobile-candidate-dispatch.md`；历史报告、旧测试与旧 ADR 不得自行产生新门禁。发现冲突须同步消除，不能只在新文档写相反结论。

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
