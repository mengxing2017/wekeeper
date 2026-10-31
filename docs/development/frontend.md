# Web 与小程序工作区

- `apps/web`：React + Ant Design 管理/运营与现有社区 Web，部署到 Caddy。
- `apps/miniapp`：uni-app + Vue 3 + TypeScript 微信小程序，产物上传微信平台。
- 根目录 `api-contract/`：唯一接口 DTO、JSON:API 协议与业务 client；不部署。
- 根目录旧 `uniapp/`：Vue 2 迁移参考，不参与安装、构建或镜像。

## 开发与验证

从仓库根运行：

```bash
make install
make check-web
pnpm --dir frontend dev
pnpm --dir frontend dev:miniapp
```

`make check-web` 包含 contract 的浏览器及无 DOM 类型检查、契约测试、双客户端类型检查/lint/测试/生产构建与导入边界检查。单独校验小程序使用 `make check-miniapp`。

Web 默认 `http://localhost:5173`，通过 Vite 代理请求 `WEKEEPER_API_ORIGIN`（默认 `http://127.0.0.1:8080`）。根 `frontend dev` 继续转发参数给 Web，例如 `pnpm --dir frontend dev --host 127.0.0.1 --port 5174`。

## 微信小程序配置

1. 将 `apps/miniapp/.env.example` 复制为同目录 `.env.local`，配置 `VITE_API_BASE_URL` 为实际 API 的 HTTPS origin，不带路径，不带凭据。这里使用与 Web 相同的 Rust API，不接 PHP 或 uniCloud。
2. 在 `apps/miniapp/src/manifest.json` 的 `mp-weixin.appid` 中设置自己的小程序 AppID。仓库不填示例真实身份，不包含 AppSecret。后端配置对应的 `WEKEEPER_WX_APP_ID`、`WEKEEPER_WX_SECRET`（准确配置名见根 env.example）。
3. 微信后台配置对应请求域名；图片通过 `uni.request` 认证读取原始字节后写入本地临时文件，私有图片不依赖浏览器 Cookie，不向第三方图片地址发送 Bearer。
4. 运行 `pnpm --dir frontend build:miniapp`，用微信开发者工具导入 `apps/miniapp/dist/build/mp-weixin`。开发模式输出位于 `dist/dev/mp-weixin`。
5. 用真实身份及隔离学校验收登录、绑定、图片选择、发布、后台审核、详情互动和弱网恢复。构建成功不表示真实微信身份或真机验收完成。

默认开启小程序域名检查，关闭 uniStatistics。生产发布与微信审核单独进行，不加入 Compose；Web Dockerfile 安装/构建 contract + web，并独立构建 docs；运行层复制 Web dist 和文档静态产物。

## 接口与边界

只有 feature 的 `api/index.ts` 导入 `@wekeeper/api-contract/*`。Web 与小程序禁止互相引用，社区不得依赖管理工作区，shared 不定义 DTO/client。`tools/check-boundaries.mjs` 检查静态导入、类型导入、re-export、动态导入和 Vue script。

平台中立 client 通过 transport 传输，统一编码 JSON:API、验证资源/分页并保留服务器错误。Web 保留 Cookie + CSRF 兼容 facade；小程序使用独立 Bearer 和可信 HTTPS origin。写入不自动重试，响应丢失时先核对“我的发布”。

小程序学校缓存、页面请求代次、私有图片与草稿按账号/学校隔离；切校取消旧请求，撤权清理页面，退出清 token 与私有图片。草稿保留于本机，只能在相同账号与学校恢复，编辑器隐藏时自动保存非空草稿，发布成功删除对应草稿。敏感终端可在丢弃草稿后退出。

## 迁移范围

保留 18 个旧注册路径，新增非 tabBar 的动态编辑页以支持带参数跳转，使用薄页面 + feature 组件组织；页面生命周期由注册页显式转发给 feature。功能包括发现/关注/收藏/搜索/我的发布、详情评论回复与举报、图片发布/编辑/删除、学校绑定/切换、昵称、通知已读、资源与反馈图片。

旧独立分类改为校园服务入口，标签页展示配置标签，不冒充服务端精确标签筛选；旧粉丝及点赞人员明细明确下线，我的关注与互动消息保留。位置、视频、手机号/头像修改、短信登录不在首期恢复。旧整数详情链接不自动猜测 UUID，需使用已验证的迁移映射后再做兼容发布。

复用旧默认主题色、双列内容卡片/图片预览等交互及默认头像素材。未整包复制旧 UI 库和混合 PHP 调用的巨型页面；Vue 2 全局注入已由 Vue 3 feature 与显式 client 代替。

工程决策见 [ADR 0006](../adr/0006-frontend-clients.md)，平台配置与部署见 [配置索引](../configuration/index.md)。
