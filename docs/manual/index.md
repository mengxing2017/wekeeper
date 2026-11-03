# 安装与使用

已部署系统的日常操作请先看[使用手册](../guide/index.md)。本页面向安装、开发和部署人员。

当前手机业务使用[手机共享发送使用手册](../guide/operator/mobile-execution-tasks.md)，办理步骤见[手机使用管理](../guide/platform-admin/shared-mobile-resources.md)和[手机任务](../guide/operator/mobile-execution-tasks.md)。

按目标选择入口，避免将临时开发数据误作生产持久数据。

| 场景 | 操作入口 | 数据与验收边界 |
| --- | --- | --- |
| 本地快速开发 | [嵌入式 PostgreSQL](./local-development.md) | 每次重启重置，自动临时管理员；忽略外部数据库配置 |
| 本地保留数据 | [外部数据库开发](./local-development.md#外部数据库开发) | 自行管理数据库与凭据，本地进程不自动读 .env |
| 镜像构建及服务器部署 | [Docker 完整手册](../runbooks/docker.md) | HTTPS、管理员初始化、持久卷、备份及更新验收 |
| Web 与小程序开发 | [前端说明](../development/frontend.md) | 独立构建，真实微信身份和真机另验收 |
| 学校运营与历史切换 | [业务运行手册](../runbooks/business.md) | 学校授权、审核、迁移及历史记录核验 |
| 企业微信接入 | [接入手册](../runbooks/wecom.md) | 官方权限、可信域名、OAuth 和真机确认 |
| 手机任务操作 | [手机共享发送使用手册](../guide/operator/mobile-execution-tasks.md) | 预览、确认、逐群执行及结果核对 |

先建立隔离开发/测试环境，再执行真实平台及生产数据切换。文档启动与应用启动分别管理，见 [开发命令](../development/index.md)。
