# Frontend 工作区

- `apps/web`：React + Ant Design 管理与转发运营，部署到 Caddy。
- 根目录 `api-contract/`：接口 DTO、JSON:API 协议与业务 client，不部署。
- 根目录旧 `uniapp/` 为历史迁移参考，不参与本项目安装、构建或镜像。

## 开发与验证

从 wekeeper 仓库根运行：

```bash
make install
make check-web
pnpm --dir frontend dev
```

`make check-web` 包含 contract 的浏览器及无 DOM 类型检查、契约测试、Web 类型检查、lint、测试、生产构建与导入边界检查。

Web 默认 `http://localhost:5173`，通过 Vite 代理请求 `WEKEEPER_API_ORIGIN`（默认 `http://127.0.0.1:8080`）。例如：`pnpm --dir frontend dev --host 127.0.0.1 --port 5174`。

## 接口与边界

只有 feature 的 `api/index.ts` 导入 `@wekeeper/api-contract/*`。shared 不定义 DTO/client；contract 引用必须匹配包声明的导出。`tools/check-boundaries.mjs` 检查静态、类型、re-export 与动态导入。

Web 使用 Cookie + Origin/CSRF。设备发送继续使用自己的独立凭据。写入不自动重试，响应丢失时先核对原任务。


群发任务直接填写正文并保存冻结快照。linjian 的开发、构建和发布请查阅其独立项目说明。
