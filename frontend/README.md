# Frontend 工作区

- `apps/web`：React + Ant Design 管理与转发运营，部署到 Caddy。
- 根目录 `api-contract/`：接口 DTO、JSON:API 协议与业务 client，不部署。
- 论坛 Web 和整个 `apps/miniapp/` 已迁入独立 linjian，本项目已删除对应源码、登录接口、脚本与依赖。
- 根目录旧 `uniapp/` 为历史迁移参考，不参与本项目安装、构建或镜像。

## 开发与验证

从仓库根运行：

```bash
make install
make check-web
pnpm --dir frontend dev
```

`make check-web` 包含 contract 的浏览器及无 DOM 类型检查、契约测试、Web 类型检查、lint、测试、生产构建与导入边界检查。

Web 默认 `http://localhost:5173`，通过 Vite 代理请求 `WEKEEPER_API_ORIGIN`（默认 `http://127.0.0.1:8080`）。例如：`pnpm --dir frontend dev --host 127.0.0.1 --port 5174`。

`@wekeeper/api-contract` 通过 `link:../../../api-contract` 直接使用仓库源码，避免本地 `file:` 安装副本滞后导致页面提示缺少导出。首次切换依赖方式后执行 `pnpm --dir frontend install --frozen-lockfile`，重启 Vite 并刷新浏览器；后续契约源码修改无需重新安装。

## 接口与边界

只有 feature 的 `api/index.ts` 导入 `@wekeeper/api-contract/*`。shared 不定义 DTO/client；禁止 import 已移除的小程序与论坛模块。`tools/check-boundaries.mjs` 检查静态、类型、re-export 与动态导入。

Web 使用 Cookie + Origin/CSRF。设备发送继续使用自己的独立凭据。写入不自动重试，响应丢失时先核对原任务。

论坛管理五个入口及其直接 URL 已退场：内容审核、评论审核、举报与反馈、展示与屏蔽词、平台默认展示配置。学校成员与权限、学校设置、学校管理和登录账号管理也已移出；保留分发登录、平台管理员与两类独立工作空间。

发送使用的历史素材通过分发服务自己的 `distribution-materials` 接口选择；后端读取 `distribution_materials`，不调用已删除的论坛帖子接口。旧 `/community` 页面返回未找到页面。linjian 的开发、构建和发布请查阅其独立项目说明。
