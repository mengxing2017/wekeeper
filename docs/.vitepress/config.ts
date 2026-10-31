import { defineConfig } from 'vitepress'

export default defineConfig({
  lang: 'zh-CN',
  title: 'WeKeeper',
  description: '校园社区与内容运营：开发、部署、业务运行与验收',
  cleanUrls: true,
  lastUpdated: true,
  themeConfig: {
    nav: [
      { text: '首页', link: '/' },
      { text: '手册', link: '/manual/' },
      { text: '开发', link: '/development/' },
      { text: '业务流程', link: '/pipeline/' },
      { text: '配置', link: '/configuration/' },
      { text: '运行', link: '/operations/' },
      { text: '参考', link: '/references/' },
    ],
    sidebar: [
      { text: '开始使用', items: [
        { text: '项目概览', link: '/' },
        { text: '安装与使用', link: '/manual/' },
        { text: '本地开发与凭据注入', link: '/manual/local-development' },
        { text: 'Docker 制作与部署', link: '/runbooks/docker' },
      ] },
      { text: '工程与配置', items: [
        { text: '开发命令与文档站', link: '/development/' },
        { text: 'Web 与小程序', link: '/development/frontend' },
        { text: '业务与发送流程', link: '/pipeline/' },
        { text: '配置索引', link: '/configuration/' },
      ] },
      { text: '运行与验收', items: [
        { text: '运行手册索引', link: '/operations/' },
        { text: '业务、迁移与验收', link: '/runbooks/business' },
        { text: '企业微信接入', link: '/runbooks/wecom' },
        { text: '手机执行接口', link: '/runbooks/mobile-delivery' },
        { text: '多应用业务配置', link: '/runbooks/automation-workflows' },
        { text: '历史企业微信规则与多群执行', link: '/runbooks/configured-mobile-delivery' },
        { text: '基础阶段验证记录', link: '/runbooks/foundation-validation' },
      ] },
      { text: '工程决策与参考', items: [
        { text: '协议与源码入口', link: '/references/' },
        { text: '基础骨架 ADR 0001', link: '/adr/0001-foundation' },
        { text: '业务迁移 ADR 0002', link: '/adr/0002-business-migration' },
        { text: '手机执行 ADR 0005', link: '/adr/0005-mobile-delivery' },
        { text: '双客户端 ADR 0006', link: '/adr/0006-frontend-clients' },
      ] },
    ],
    search: { provider: 'local' },
    outline: { label: '本页目录', level: [2, 3] },
    docFooter: { prev: '上一页', next: '下一页' },
    lastUpdated: { text: '最后更新' },
    footer: { message: '当前行为以实现与实际验收为准；历史 ADR 和验证记录保留其阶段边界。' },
  },
})
