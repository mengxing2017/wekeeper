---
layout: home
hero:
  name: WeKeeper
  text: 校园社区与内容运营
  tagline: Rust API、React Web 与 uni-app Vue 3 小程序
  actions:
    - theme: brand
      text: 安装与使用
      link: /manual/
    - theme: alt
      text: 开发入口
      link: /development/
features:
  - title: 身份与学校边界
    details: 账号、成员、内容、审核和运营操作按身份与学校授权，不以平台身份替代学校权限。
  - title: 多客户端共用协议
    details: Web 与小程序独立构建，经 feature API 使用统一 JSON:API contract，凭据按平台隔离。
  - title: 受控内容发送
    details: 官方任务与独立手机执行遵循人工确认、共享额度、去重和 unknown 核对，不自动补发。
---

## 文档定位

这是 WeKeeper 项目内的正式文档站，解释当前开发、配置、部署、业务操作及验收。历史方案和实施报告仍放在 report，不进入默认文档站；ADR 和验证记录保留各自日期及阶段语义。

部署只有 Rust API 与 Web 两个应用，Compose 为 `api/web/postgres`。小程序发布到微信平台；文档站是独立开发工具，不新增生产部署服务。旧 PHP、根 uniapp Vue 2、Java 等资源保留用于迁移核对，不进入新应用镜像。

## 关键入口

| 目标 | 入口 |
| --- | --- |
| 从零选择开发或部署方式 | [安装与使用](./manual/index.md) |
| 无 Docker 启动、企业微信凭据注入 | [本地开发](./manual/local-development.md) |
| 镜像打包、交付、HTTPS、备份回退 | [Docker 部署手册](./runbooks/docker.md) |
| 开发命令及 Web/小程序工作区 | [开发](./development/index.md)、[前端](./development/frontend.md) |
| 社区、官方任务与手机执行流程 | [业务流程](./pipeline/index.md) |
| 环境变量和配置来源 | [配置索引](./configuration/index.md) |
| 业务迁移、平台验收和故障 | [运行手册索引](./operations/index.md) |
| 契约与工程决策 | [参考资料](./references/index.md) |

## 启动文档

在 `wekeeper/` 根目录执行：

```bash
make docs-install
make docs
```

访问 <http://127.0.0.1:22596/>。只看文档需要 Node 22 和 pnpm 11.22.0，不需要 Rust、数据库、Docker 或平台凭据。更多命令见 [文档站](./development/index.md#文档站)。
