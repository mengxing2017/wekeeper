# 测试与验收

适合开发、测试及发布人员。所有数据库验证只允许显式隔离的 `WEKEEPER_TEST_DATABASE_URL`，不能回退到生产配置。

## 常规检查

```bash
make check-web
devbox run -- make check-backend
make docs-install
make docs-build
```

`check-web` 检查 contract、Web 类型、导入边界、lint、测试与构建；后端检查格式、锁文件构建、Clippy 和普通测试。数据库集成测试需提供隔离 URL 后运行 `make test-integration`；无需 Docker 的临时 PostgreSQL 生命周期用 `devbox run -- make test-embedded`。

前端使用 Node.js 26，单独运行 `pnpm --dir frontend test`，或使用项目环境执行 `devbox run -- pnpm --dir frontend test`。测试环境使用 JSDOM 31，保留真实组件、样式与可见性检查；通过 Ant Design 的 `motion: false` 关闭 JSDOM 不执行的动画，单项测试超时为 15 秒。

Devbox 使用独立的 pnpm 包，显式禁用 Corepack，避免用户环境开启的旧 Corepack 入口覆盖项目工具。修改环境后退出并重新进入 `devbox shell`；`node --version` 应为 26，`command -v pnpm` 应指向 `.devbox/nix/profile/default/bin/pnpm`。

来源接入集成测试还需测试用的 `WEKEEPER_SOURCE_MASTER_KEY_V1`（32 随机字节的无填充 base64url），手机执行包测试还需 `WEKEEPER_SHARED_SEND_KEY`（32 随机字节的 64 位十六进制）。在隔离测试进程中生成独立密钥，不读取生产密钥；只设置数据库 URL 不足以运行全部集成测试。

## 文档及同镜像检查

`make docs-build` 对 Markdown 死链、静态资源和脚本 CSP 进行构建验证。新增后台页面时同步更新 `app/shell/help.ts` 与手册，帮助映射测试保证每个菜单均有固定目标。

Compose 构建启动后运行 `make smoke`。该检查含 API、SPA、文档 clean URL、文档静态资源和真实 404；仍需浏览器核验搜索、主题、代码复制、移动导航及 CSP 控制台。

## 不能替代的验收

模拟平台、TypeScript/Rust 契约和数据库测试不等于真实平台权限、手机原生适配或送达证明。设备 WebSocket 需用有效专用凭据握手并往返帧。真机发送先选择隔离测试环境和授权测试群，逐项验证等待、断网、停止、重启和 unknown，保留证据。

文档交接由业务人员按[每日清单](../guide/operator/index.md)、管理员按[上线清单](../guide/platform-admin/index.md)独立操作，记录卡住的位置再修订。无法执行的环境验证要在发布记录中明确标为未验收。

业务单位、广告与来源链路可使用 `devbox run -- cargo run --locked --manifest-path backend/Cargo.toml --example test_business_unit_identity` 在临时 PostgreSQL 17 中验证。入口向测试进程显式注入 `WEKEEPER_TEST_DATABASE_URL` 和独立测试密钥，运行结束删除临时数据。仅重跑来源链路可在命令末尾添加 `-- --source-routing`。
