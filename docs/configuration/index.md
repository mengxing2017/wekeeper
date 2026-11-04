# 配置索引

当前手机业务使用[手机共享发送使用手册](../guide/operator/mobile-execution-tasks.md)，办理步骤见[手机使用管理](../guide/platform-admin/shared-mobile-resources.md)和[手机任务](../guide/operator/mobile-execution-tasks.md)。

## 配置来源

| 文件/入口 | 职责 |
| --- | --- |
| `env.example` | Compose 环境变量模板，复制后替换实际值 |
| `compose.yaml` | 运行已发布镜像的三服务生产编排 |
| `compose.dev.yaml` | 源码构建和本地环回调试端口 overlay |
| `backend/` 配置解析 | API 实际读取与校验环境变量 |
| `frontend/apps/web/vite.config.ts` | Web 开发端口与 API 代理 |
| `frontend/apps/miniapp/.env.example` | 小程序公开 API origin 模板，不含后端 secret |
| `frontend/apps/miniapp/src/manifest.json` | 小程序 AppID 与平台构建配置 |
| `devbox.json` | Rust/Node/pnpm 开发工具环境 |
| `docs/package.json` / `.vitepress/config.ts` | 文档脚本、页面导航与本地搜索 |
| 平台「手机群消息使用管理 → 配置业务方案」 | 当前 v9 发送开关、时区、时段及四维业务额度；独立于产品技术交付 |

具体默认值和完整部署命令见 [Docker 配置手册](../runbooks/docker.md)，不要用不同文档中的示例覆盖已有生产配置。


## 变量分组

| 变量 | 用途与范围 |
| --- | --- |
| `POSTGRES_*` | Compose 数据库初始化；修改 env 不会自动更改已有卷中的密码 |
| `WEKEEPER_DATABASE_*` | 普通 API 数据库连接；本地进程需显式注入 |
| `WEKEEPER_PUBLIC_ORIGIN` | 浏览器 Origin 校验与公开链接，生产使用实际 HTTPS origin |
| `WEKEEPER_INSECURE_LOCAL_AUTH` | 仅本地 HTTP 登录；生产 false，嵌入式入口强制本地模式 |
| `WEKEEPER_WECOM_CORP_ID` / `WEKEEPER_WECOM_SECRET` | 单企业后端凭据，不能注入前端 |
| `WEKEEPER_WECOM_AGENT_ID` | 旧配置识别兼容；当前目录、群发与 OAuth 不要求 |
| `WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS` / `WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS` | 周期同步；0 关闭周期调度，不关闭手工同步 |
| `WEKEEPER_WX_APP_ID` / `WEKEEPER_WX_SECRET` | 后端微信身份接入，Secret 不放入小程序包 |
| `WEKEEPER_SHARED_SEND_KEY` | 共享手机发送恢复材料的后端加密密钥；`openssl rand -hex 32` 生成，跨重启/实例保持一致并配套备份；仅注入 API，详见 [Docker 配置](../runbooks/docker.md#共享手机发送密钥-首次配置与已有部署更新) |
| `WEKEEPER_TEST_DATABASE_URL` | 集成测试专用隔离库，不回退生产地址 |
| `WEKEEPER_API_ORIGIN` | Vite 开发代理的 API 目标 |
| `VITE_API_BASE_URL` | 小程序公开 HTTPS API origin，不含凭据和路径 |

## 加载规则

Compose 读取 `.env` 并按编排注入；普通本地 API 和嵌入式入口不自动加载 `.env`。嵌入式入口忽略外部数据库配置，但继承导出的企业微信配置。具体注入见 [本地开发](../manual/local-development.md#企业微信凭据注入)。

Secret、数据库密码和设备 token 不进入 VITE 变量、日志或文档仓库。修改配置后按运行方式重启/重建进程；仅 `docker restart` 不会重新读取 env 文件。数据库卷、发送历史与 unknown 不能用清库方式处理配置错误。
