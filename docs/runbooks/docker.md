# Docker 部署与运维

## 1. 文件职责

- compose.yaml：运行已发布镜像，仅 web 公开入口；默认绑定 127.0.0.1。
- compose.dev.yaml：源码构建，以及数据库/API 的 127.0.0.1 调试端口。
- env.example：变量模板，不含有效业务密钥。
- backend/Dockerfile：Rust 多阶段构建、固定工具链、非 root 运行。
- frontend/Dockerfile：构建 React/contract，Caddy 提供页面及同源反向代理。

Compose 命令显式使用 --env-file .env；不要在运行日志中输出 `docker compose config` 的完整内容，它包含数据库密码。
语法检查使用 `docker compose ... config --quiet`。

## 2. 本地或入口代理终止 TLS

默认配置只允许宿主机访问 http://127.0.0.1:8088，适合调试或同机 Nginx/1Panel 反向代理。
入口代理将公开 HTTPS 流量转发到 127.0.0.1:8088，并保留 Host 与转发协议头。
跨宿主机代理时，显式修改 WEB_BIND_HOST 并限制来源网络，不公开数据库或 API 端口。
WEKEEPER_WEB_LISTEN 保持 :80，避免与平台同时申请证书。
正式身份/企业微信接入前必须完成公开域名、HTTPS 和可信回调配置。

## 3. Caddy 直接终止 TLS

域名 DNS 指向服务器，开放 80/443，并配置：

```dotenv
WEB_BIND_HOST=0.0.0.0
WEB_HTTP_PORT=80
WEB_HTTPS_PORT=443
WEKEEPER_WEB_LISTEN=keeper.example.com
```

使用自己的真实域名；Caddy 自动管理证书，持久化在 caddy-data/caddy-config。
此模式 /healthz 会受域名与 HTTPS 规则约束；web 容器自身检查独立的内部 8081 端口，不公开该端口。
端到端验证设置 `WEKEEPER_SMOKE_ORIGIN=https://真实域名` 后运行 `make smoke`。
不要使用 -k 跳过生产证书验证。

## 4. 数据与配置

postgres-data 保存 PostgreSQL 数据；更改 POSTGRES_PASSWORD 不会自动修改已初始化数据库中的角色密码。
轮换需先在数据库内受控修改密码，再更新 .env 并重建 api；避免在历史中写明文密码。
不要直接将 PostgreSQL 17 的数据卷挂载到不同主版本，升级需独立迁移计划。
开发和生产使用不同 Compose project、环境文件和持久卷，不能混用。
镜像不包含 .env、历史上传、PHP、Java、千寻等内容。
未来媒体存储需显式新增持久化配置，本骨架尚不接受文件上传。

## 5. 出站网络

api 连接 database 与 outbound 两个网络；outbound 允许容器通过宿主机访问官方 API。
postgres 仅连接 internal database 网络。网络拓扑不等于已验证企业出口策略。
接入腾讯官方能力时，从 api 容器实测 DNS、HTTPS、证书链、代理和超时。
当前骨架没有向腾讯发起请求，不需要企业微信凭证即可启动。

## 6. 备份与恢复

定期执行 PostgreSQL 逻辑备份，并将备份移出单一服务器、限制访问。
示例将备份写到受控宿主机路径；操作前确认当前 .env 对应正确环境：

```bash
docker compose --env-file .env exec -T postgres sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" -Fc' > wekeeper-backup.dump
```

备份可能包含用户数据。正式部署后建立加密、保留和恢复演练制度。
恢复应在新建的隔离数据库执行，不向正在运行的生产库直接覆盖导入。
上线前确认镜像版本、migration 版本与备份时间可以对应追溯。

## 7. 发布与回滚

1. 运行质量检查和隔离集成测试，构建不可变版本镜像。
2. 对目标数据库备份，检查本次 migration 的兼容性。
3. 更新 API_IMAGE/WEB_IMAGE，执行 pull 与 up -d --wait。
4. 检查日志、readiness、页面入口和 HTTP 冒烟。
5. 记录 commit、digest、migration 及结果。

SQL migration 为 forward-only，镜像回退需要确认旧 API 能读取新 schema。
不能仅修改标签就声称数据已回滚；真实业务开始写入后遵循实施方案的停写/核对流程。

## 8. 排障

```bash
docker compose --env-file .env ps
docker compose --env-file .env logs --tail 100 api web
curl --fail http://127.0.0.1:8088/readyz
```

- PostgreSQL 不健康：核对密码、卷、磁盘与初始化日志。
- api 启动失败：核对数据库连通与 migration；不绕过 migration 强行启动。
- readyz 返回 503：检查数据库连接及 application_metadata，不向客户端暴露原始错误。
- 页面可以打开但 API 失败：检查 Caddy 上游与 api 健康，而不是用页面 200 代替就绪检查。
- /api/v1/posts 返回 404：当前属于预期行为，业务模块未实现。
- H5 没有发送按钮：当前属于预期行为，身份、学校与 SDK 接入尚未实施。
