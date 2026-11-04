# 手机旧通道移除与能力归并

2026-11-04 起，手机发送只有当前共享手机发送业务。旧 v1–v5 发送、配置、账号与学校执行，以及 v6/v7 维护验证和能力审批接口均已移除，旧 URL 返回 404，不再以 409 退场开关维持第二套业务。

## 当前代码归属

- `distribution/domain/device_registry.rs`、对应 local/port/postgres/remote：设备注册、查询、启停、配对、防猜预算和凭据代次。
- `distribution/domain/shared_mobile/`：独立目标群、共享绑定、FlowV3、实际规则校验和使用管理模型。
- `distribution/*/shared_mobile_resources.rs`：目标群和共享手机绑定管理。
- `shared_send_v2`：当前 v9 任务、预览确认、领取、检查、单次调用、结果补报、unknown 核对、独立收件同步与可靠唤醒。
- `device_control`：原在线状态、心跳、重连、控制命令与两阶段凭据续期。

管理设备仍使用 `/api/v1/platform/execution-devices`，这是有效管理 API，并非旧发送协议。手机新配对和轮换改为 `/api/v9/device-execution/enrollments`、`credential-rotations`，使用 JSON:API `device-credentials`。

keeper-otter 配对通过独立 DeviceRegistryGateway，不再使用旧 Claim/Permit 网关。已有本地配置内部更新为协议 9，保留设备 ID、token、凭据代次、托管同意、共享任务、结果补报、消费墓碑与在线控制状态，不要求清库或重新配对。旧本地记录只用于历史查阅，不会变成新的发送请求。

## 数据升级

前向迁移 `202611040124_retire_legacy_mobile_channels.sql` 显式删除 103 张旧表及对应函数，不使用 CASCADE，不修改已执行的基线 migration。当前资源操作与审计表更名后保留原数据。设备协议元数据更新为 9，不递增凭据代次、不撤销凭据或当前同意。

旧通道如仍有在途调用或 unknown，只将相关记录保存到 `retired_mobile_incident_archive` 只读归档，不生成新任务或发送许可。该归档不参与调度，不增加管理员办理流程。

发布需同步升级后端和手机端。旧手机配对 URL 不再保留；既有手机身份仍有效。数据库升级后不得回退到查询已删除表的旧后端。

## 验证

`backend/tests/device_registry.rs` 覆盖独立生命周期和幂等、一次性配对、轮换、撤销；`http_contract.rs` 验证旧端点不存在和新入口身份边界；`database.rs` 与 `tests/sql/legacy_mobile_retirement.sql` 验证已有凭据、同意、v9 计划/任务/唤醒及官方客户群保留。

数据库测试仅使用显式隔离的 `WEKEEPER_TEST_DATABASE_URL`。真机验收仍需核对已有设备升级、配对、长期托管、重连、任务同步和实际企业微信 UI 操作。
