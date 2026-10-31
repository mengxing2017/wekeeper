---
layout: home
hero:
  name: WeKeeper 帮助中心
  text: 先找到自己的工作，再按步骤完成
  tagline: 使用手册、开发者手册与部署说明随当前系统一同发布
  actions:
    - theme: brand
      text: 第一次使用
      link: /guide/quick-start
    - theme: alt
      text: 遇到问题
      link: /guide/troubleshooting
features:
  - title: 业务人员
    details: 从审核、客户群和消息准备开始，了解确认与发送结果。
    link: /guide/operator/
    linkText: 每日工作清单
  - title: 学校管理员
    details: 核对本校设置、成员、客户群与发送范围。
    link: /guide/school-admin/
    linkText: 学校准备清单
  - title: 平台管理员
    details: 从账号、学校和授权开始，准备通讯录、群目录与手机。
    link: /guide/platform-admin/
    linkText: 平台上线清单
  - title: 开发与部署
    details: 查看架构、契约、测试、Docker 和故障处理。
    link: /development/
    linkText: 开发者手册
---

## 我应该看哪一份手册？

日常业务从[使用手册](./guide/index.md)开始；服务器部署看[运行与故障处理](./operations/index.md)；修改代码看[开发者手册](./development/index.md)。不理解菜单或任务状态时先查[术语](./guide/glossary.md)。

当前手机新任务使用[通用流程 v5](./runbooks/dispatch-v5.md)。历史 v3/v4 文档仅适用于对应旧任务。官方群发与手机发送是不同操作路径，创建成功不等于已送达，结果不确定须在原任务人工核对。

## 阅读与维护

文档和 Web 随同一个镜像部署在 `/docs/`，不新增服务，无需登录即可阅读通用资料。顶栏“返回工作台”可回到业务系统。文档不包含实际账号密码、设备凭据或生产客户资料。

维护文档使用 `make docs-install`、`make docs`，访问 `http://127.0.0.1:22596/docs/`。写作与发布见[文档维护手册](./development/docs-publishing.md)。
