# 文档写作、构建与发布

文档使用独立 VitePress 包，和 React Web 分别构建，静态产物进入同一个 Web 镜像。Compose 仍是 api/web/postgres；文档阅读和本地搜索不依赖 API 或登录。

## 本地命令

```bash
make docs-install
make docs
# http://127.0.0.1:22596/docs/
make docs-build
make docs-preview
# http://127.0.0.1:22597/docs/
```

部署和本地保持 `/docs/` base。构建产物为 `docs/.vitepress/dist`，生成的 CSP 片段在 `docs/.vitepress/caddy/docs.caddy`；这两个目录及依赖、缓存不提交版本库。文档源文件、图片、构建脚本与锁文件必须提交。

## 新增手册

1. 在 `guide/` 按角色和任务增加 Markdown；写清适用角色、前置条件、真实菜单/按钮、步骤、可观察结果和故障下一步。
2. 在 `.vitepress/config.ts` 对应侧栏登记；旧 runbooks/ADR URL 保留，历史流程在页首标记适用范围。
3. 用当前界面与隔离示例数据生成截图，放在 `public/images/guide/`，加说明和替代文字；不要手绘假截图或泄露业务数据。
4. 在 Web `app/shell/help.ts` 登记页面帮助。文档入口使用普通链接，不能由 React Router 拦截，也不能附带学校或设备凭据。
5. 执行文档构建及相关帮助入口测试。死链是构建错误，不允许打开忽略死链绕过。

## 镜像与版本

`frontend/Dockerfile` 的 docs-builder 使用独立 frozen-lockfile 安装并构建；runtime 将产物复制到 `/srv/docs`，将 CSP 片段复制到 `/etc/caddy/docs.caddy`。使用构建参数记录发布版本，例如：

```bash
docker build -f frontend/Dockerfile --build-arg WEKEEPER_BUILD_VERSION=release-example -t wekeeper-web:example .
```

未指定版本显示“开发构建”，不依赖 `.git` 或编造更新时间。前端回退镜像时文档同时回退。文档编译失败会阻止镜像发布，禁止使用旧产物继续打包。

## 路由、缓存与 CSP

Caddy 为 `/docs` 补尾斜杠，对 `/docs/*` 匹配实际文件、`.html` 和目录 `index.html`；不存在文章返回文档 404 与 HTTP 404，不回退到 React 或帮助首页。

HTML 与普通资源可重新验证，带 hash 的文档 assets 长期缓存。构建脚本从 HTML 内联脚本生成 SHA-256 白名单，仅对文档响应应用，业务页继续使用原 CSP。脚本变化必须重新构建片段，不能全站开放 unsafe-inline 脚本。

文档是公开通用资料，隐藏开发导航不是鉴权。生产 Secret、设备 token、真实客户数据和内部环境细节不得进入 Markdown、图片或搜索索引。需内部认证的资料应另设受保护发布方案。

## 发布检查

可用已安装 Chromium 运行浏览器检查（不安装新的应用依赖）：

```bash
CHROME_EXECUTABLE=/path/to/chrome WEKEEPER_SMOKE_ORIGIN=http://127.0.0.1:8088 node tools/check_docs_browser.mjs
```

构建镜像时也会运行 Caddy 配置校验。

运行 `make smoke` 检查部署路由，再用浏览器验证本地搜索、深链接刷新、返回工作台、主题切换和代码复制。新增图片时检查资源路径；手机设备通道还需有效隔离凭据的真实 WebSocket 验证，文档发布不能改变 `/api/*` 代理行为。
