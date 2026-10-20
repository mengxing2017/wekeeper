# 骨架交付验证记录

日期：2026-10-20。

## 已完成

| 检查 | 结果 |
| --- | --- |
| Rust 1.97.1 编译 | 通过 |
| cargo fmt --check | 通过 |
| cargo clippy --all-targets -- -D warnings | 通过 |
| Rust HTTP / 契约测试 | 4 项通过 |
| TypeScript contract 类型检查 | 通过 |
| contract HTTP client 测试 | 4 项通过 |
| React TypeScript 与 ESLint | 通过 |
| React 行为测试 | 3 项通过 |
| React 生产构建 | 通过 |
| 隔离 PostgreSQL 17.11 集成测试 | 1 项通过，migration 执行两次且 readiness 成功 |
| 真实 API + Vite 同源代理冒烟 | 通过，包含 /admin、/wecom、healthz、readyz、system 与业务路径 404 |
| API SIGTERM 优雅停止 | 退出码 0 |
| 嵌入式 PostgreSQL 开发入口 | 通过，共享正式迁移与 API，忽略无效外部数据库配置 |
| 嵌入式 SIGINT / SIGTERM 生命周期 | 均通过，API 退出码 0，数据库端口关闭、临时数据目录删除 |
| 嵌入式 HTTP 端口冲突 | 错误退出，数据库停止且临时数据目录删除 |
| Compose YAML 语法及网络拓扑静态检查 | 通过，只有 web/api/postgres，数据库内部网络、API 另有出站网络 |
| Shell 语法与补丁空白检查 | 通过 |

数据库与本地 HTTP 验证使用临时目录、动态环回端口与空白测试数据库。
测试结束已停止测试服务并清理临时数据，未连接或修改旧 MySQL。
嵌入式生命周期检查使用 `devbox run -- make test-embedded`，已加入 CI；信号自动检查面向 POSIX 环境。
依赖锁文件已生成：backend/Cargo.lock、frontend/pnpm-lock.yaml、api-contract/pnpm-lock.yaml。

## 尚未验证

当前环境没有 Docker 命令或 Docker daemon，未在本机实际构建镜像或启动三容器栈。
Compose 静态检查不能替代镜像构建、Caddy 运行与容器网络验证。
CI 已配置 Docker 构建、Compose --wait 与 tools/smoke.sh，需在有 Docker 的 runner/服务器运行。
本次没有触发远程 CI，没有推送镜像或进行生产部署。

企业微信 OAuth、官方 SDK、学校权限、模板和真实发送属于后续业务阶段，本次未接入或测试。
