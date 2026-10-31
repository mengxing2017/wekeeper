import { defineConfig } from "vitepress";

export default defineConfig({
  base: "/docs/",
  lang: "zh-CN",
  title: "WeKeeper",
  description: "校园社区与内容运营：开发、部署、业务运行与验收",
  cleanUrls: true,
  lastUpdated: false,
  themeConfig: {
    nav: [
      { text: "使用手册", link: "/guide/" },
      { text: "开发者手册", link: "/development/" },
      { text: "部署与运维", link: "/operations/" },
      { text: "参考资料", link: "/references/" },
    ],
    sidebar: {
      "/guide/": [
        {
          text: "开始使用",
          items: [
            { text: "选择你的角色", link: "/guide/" },
            { text: "第一次登录", link: "/guide/quick-start" },
            { text: "角色与学校", link: "/guide/roles-and-schools" },
            { text: "术语与状态", link: "/guide/glossary" },
            { text: "常见问题", link: "/guide/troubleshooting" },
          ],
        },
        {
          text: "业务人员",
          items: [
            { text: "每日工作清单", link: "/guide/operator/" },
            { text: "内容、评论与举报", link: "/guide/operator/moderation" },
            {
              text: "客户群与模板",
              link: "/guide/operator/groups-and-templates",
            },
            { text: "官方群发任务", link: "/guide/operator/broadcast-tasks" },
            { text: "手机发送任务", link: "/guide/operator/mobile-tasks" },
            {
              text: "结果与异常处理",
              link: "/guide/operator/results-and-recovery",
            },
          ],
        },
        {
          text: "学校管理员",
          items: [
            { text: "学校准备清单", link: "/guide/school-admin/" },
            {
              text: "学校设置与成员",
              link: "/guide/school-admin/settings-and-members",
            },
            {
              text: "发送前的准备",
              link: "/guide/school-admin/delivery-setup",
            },
          ],
        },
        {
          text: "平台管理员",
          items: [
            { text: "平台上线清单", link: "/guide/platform-admin/" },
            {
              text: "账号、学校与授权",
              link: "/guide/platform-admin/accounts-and-schools",
            },
            {
              text: "通讯录与客户群目录",
              link: "/guide/platform-admin/directory-and-assets",
            },
            {
              text: "手机设备与发送账号",
              link: "/guide/platform-admin/devices-and-accounts",
            },
            {
              text: "流程维护与发布",
              link: "/guide/platform-admin/flow-maintenance",
            },
            {
              text: "审计与公共资源",
              link: "/guide/platform-admin/audit-and-resources",
            },
          ],
        },
      ],
      "/development/": [
        {
          text: "开发者手册",
          items: [
            { text: "开发入口", link: "/development/" },
            { text: "本地开发", link: "/manual/local-development" },
            { text: "Web 与小程序", link: "/development/frontend" },
            { text: "Rust 后端", link: "/development/backend" },
            { text: "协议与授权", link: "/development/contracts-and-auth" },
            { text: "测试与验收", link: "/development/testing" },
            { text: "文档构建与发布", link: "/development/docs-publishing" },
          ],
        },
      ],
      "/": [
        {
          text: "部署与参考",
          items: [
            { text: "帮助首页", link: "/" },
            { text: "使用手册", link: "/guide/" },
            { text: "安装与使用", link: "/manual/" },
            { text: "部署与运维", link: "/operations/" },
            { text: "Docker 部署", link: "/runbooks/docker" },
            { text: "配置索引", link: "/configuration/" },
            { text: "业务流程", link: "/pipeline/" },
            { text: "业务运行", link: "/runbooks/business" },
            { text: "企业微信接入", link: "/runbooks/wecom" },
            { text: "当前手机任务 v5", link: "/runbooks/dispatch-v5" },
            { text: "历史手机接口", link: "/runbooks/mobile-delivery" },
            { text: "历史 v4 工作流", link: "/runbooks/automation-workflows" },
            {
              text: "历史 v3 配置",
              link: "/runbooks/configured-mobile-delivery",
            },
            { text: "协议与 ADR", link: "/references/" },
          ],
        },
      ],
    },
    search: { provider: "local" },
    outline: { label: "本页目录", level: [2, 3] },
    docFooter: { prev: "上一页", next: "下一页" },
    lastUpdated: { text: "最后更新" },
    footer: {
      message: "手册与应用随同一镜像发布；真实平台和手机能力以实际验收为准。",
      copyright: `版本：${(process.env.WEKEEPER_BUILD_VERSION || "开发构建").replace(/[&<>\"']/g, "")}`,
    },
  },
});
