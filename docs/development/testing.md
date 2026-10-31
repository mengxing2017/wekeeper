# 测试与验收

适合开发、测试及发布人员。所有数据库验证只允许显式隔离的 `WEKEEPER_TEST_DATABASE_URL`，不能回退到生产配置。

## 常规检查

```bash
make check-web
devbox run -- make check-backend
make docs-install
make docs-build
```

`check-web` 检查 contract、Web/小程序类型、导入边界、lint、测试与构建；后端检查格式、锁文件构建、Clippy 和普通测试。数据库集成测试需提供隔离 URL 后运行 `make test-integration`；无需 Docker 的临时 PostgreSQL 生命周期用 `devbox run -- make test-embedded`。

## 文档及同镜像检查

`make docs-build` 对 Markdown 死链、静态资源和脚本 CSP 进行构建验证。新增后台页面时同步更新 `app/shell/help.ts` 与手册，帮助映射测试保证每个菜单均有固定目标。

Compose 构建启动后运行 `make smoke`。该检查含 API、SPA、文档 clean URL、文档静态资源和真实 404；仍需浏览器核验搜索、主题、代码复制、移动导航及 CSP 控制台。

## 不能替代的验收

模拟平台、TypeScript/Rust 契约和数据库测试不等于真实平台权限、手机原生适配或送达证明。设备 WebSocket 需用有效专用凭据握手并往返帧。真机发送先选择隔离学校和授权测试群，逐项验证等待、断网、停止、重启和 unknown，保留证据。

文档交接由业务人员按[每日清单](../guide/operator/index.md)、管理员按[上线清单](../guide/platform-admin/index.md)独立操作，记录卡住的位置再修订。无法执行的环境验证要在发布记录中明确标为未验收。
