# 运行与故障处理

保留现有 runbooks 路径，按操作目标选择手册：

| 手册 | 适用任务 |
| --- | --- |
| [Docker 完整手册](../runbooks/docker.md) | 构建、发布、离线交付、HTTPS、管理员、备份、更新与故障 |
| [业务运行、迁移与验收](../runbooks/business.md) | 学校、账号、内容审核、旧数据迁移和业务闭环 |
| [企业微信接入](../runbooks/wecom.md) | 目录和群发权限、人工确认、unknown、真实平台验收 |
| [手机执行接口](../runbooks/mobile-delivery.md) | 配对、设备凭据、版本审批、permit、结果和通道切换 |
| [基础阶段验证记录](../runbooks/foundation-validation.md) | 2026-10-20 骨架检查及当时的环境限制，不是当前全部业务的验收结论 |

## 常用检查

API `/healthz` 检查存活，`/readyz` 实际检查 PostgreSQL 与 schema；迁移失败不能绕过启动。生产入口通常经 Web 同源代理 API，不单独公开数据库端口。

`make smoke` 检查 API 与 SPA HTML，不能替代浏览器执行、真实平台权限和真机测试。发布前保存提交、镜像 digest、migration 版本及对应验收结果。

## 回退与数据

数据库迁移 forward-only。旧镜像兼容性必须与已执行迁移一起评估，不兼容时不能只回退 binary。备份恢复在隔离库演练，不覆盖正在写入的生产库；保留持久卷，不执行带卷删除的清理。

外部消息不会随数据库回退而撤回。发送 unknown 保留并人工核对，关闭开关仅阻止约定的新调用，不代表已撤回官方任务或手机动作。切通道前处置在途与 unknown。

## 环境限制

文档站 `listen EPERM` 表示环境禁止本地监听，见 [文档启动说明](../development/index.md#文档站)。Docker daemon、外网或本地数据库 socket 不可用时，静态检查不能冒充实际部署/数据库检查通过。
