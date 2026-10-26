# ADR 0006：React Web 与 uni-app Vue 3 客户端分离

日期：2026-10-26。状态：已实施代码，微信真实身份、真机及发布验收待完成。

## 决策

`frontend/` 为 pnpm 工作区，包含 React 管理/运营 Web `apps/web/` 与 uni-app Vue 3 + TypeScript 微信小程序 `apps/miniapp/`。保留根目录旧 `uniapp/` 作为 Vue 2 迁移来源，不引入 Taro 或 uni-app x。两客户端独立构建，禁止互相 import。

部署仍只有 Rust API 与 Web 两个应用，Compose 仍为 web/api/postgres。小程序上传微信平台；Web 镜像只构建 contract + web，运行层只包含 Web 静态制品。原 Web 路由和开发命令兼容。

## 契约与平台

根 `api-contract/` 保持唯一 DTO/业务 client 来源。纯协议与实例化 client 通过 transport 工作，无 DOM 类型检查保证它们不依赖 fetch、Blob、Response 或 AbortSignal。旧 Web API facade 保留，使用 Cookie、Origin 与 CSRF；小程序 client 使用可信 HTTPS origin 和独立 Bearer 会话。业务请求不自动重试。

只有 feature 的 `api/index.ts` 可以引用 contract。身份、学校上下文、社区、媒体、资源与反馈为独立 feature；shared 不保存 DTO/client。Web 的 SessionGate 和纯显示工具已移出管理工作区。导入检查覆盖 TS/TSX、Vue script、类型导入、re-export 与动态导入。

## 小程序范围

保留旧 18 个注册路径，以薄页面组合 Vue 3 feature；页面生命周期在注册页转发，子组件不假设自动接收页面生命周期。复用旧默认主题色、内容双列/图片预览交互及头像素材。旧 UI 库逐项核对，不整包引入与 PHP 逻辑耦合的代码。

图片以 ArrayBuffer 上传至既有原始字节媒体入口；显示私有图片时通过相同 Bearer client 下载到本地文件，不改变后端授权和媒体协议。切校、撤权与退出清理页面数据及临时图片，并取消旧请求、拒绝迟到响应。草稿按账号/学校/帖子隔离。

已迁移发现、关注流、收藏、搜索、我的发布、详情、评论/回复、举报、图片发布/编辑/删除、学校、昵称、通知已读、资源与图片反馈。旧独立分类合并为校园服务，标签为展示；粉丝及点赞人员明细明确下线。位置、视频、头像/手机号修改、短信登录及未验证的整数分享链接不在首期恢复。

## 验证

`make check-web` 包含 contract（含无 DOM 检查）、双客户端类型/lint/测试/构建及边界检查。小程序登录响应使用同一 fixture 在 Rust 实际响应序列化与 TS client 测试中校验。数据库业务测试额外验证 Bearer/Cookie 会话不混用，以及小程序原始字节图片上传下载。

真实微信 code、域名证书、媒体真机表现、微信审核与生产灰度独立验收。运行方式见 [frontend README](../../frontend/README.md)。
