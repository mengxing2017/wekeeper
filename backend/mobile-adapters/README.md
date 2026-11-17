# 手机执行适配

手机端负责群身份识别、界面流程和本地执行适配。平台下发群名、正文、单次执行授权及业务调度约束，不管理、安装、校验或下发手机 UI flow。

设备通过认证 WSS 控制连接接收完整签名执行包，持久化后直接本地执行。配置或适配缺失由设备按本次尝试回报失败，不阻止平台创建、预览、确认或受理任务。

研发验证与发布流程不构成业务发送前置条件。数据库中的适配、环境及检查记录不提供独立的业务放行门禁。

当前执行协议与调度边界见[手机候选调度 ADR](../../docs/adr/0008-mobile-candidate-dispatch.md)，操作说明见[手机任务](../../docs/guide/operator/mobile-execution-tasks.md)和[共享手机资源管理](../../docs/guide/platform-admin/shared-mobile-resources.md)。
