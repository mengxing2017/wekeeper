# ADR 0001：Rust + React 基础骨架

日期：2026-10-20。状态：基础阶段已采用，部分设计已被后续业务实现替代。

> 本文保留初始骨架决策，不作为当前接口和认证说明。当前业务协议已统一 JSON:API，身份、学校及发送边界见 [协议参考](../references/index.md)、[ADR 0002](0002-business-migration.md) 和 [ADR 0005](0005-mobile-delivery.md)。

## 应用边界

backend 与 frontend 是两个部署应用；api-contract 是纯 TypeScript 源码包。
Rust 一个 package，按领域模块隔离；不引入 workspace 多 crate、worker、Redis 或消息队列。
使用公开 crates.io 依赖，不引用参考项目私有 registry 或地图/OTA 业务代码。

## HTTP 契约

新接口前缀 /api/v1；采用简单 JSON envelope，成功为 `{ "data": ... }`，失败为 `{ "error": { "code", "message" } }`。
暂不实现 JSON:API 关系与资源协议；旧端 code/msg/data 将来放独立 legacy 适配层。
healthz 与 readyz 为基础设施端点，使用 `{ "status": ... }`，不套业务 envelope。
readiness 查询 PostgreSQL application_metadata 的 schema_generation，超时或缺失返回 503。
不向客户端暴露数据库错误、连接信息或内部配置。
request_id 用于请求关联；当前无认证 Cookie、CORS、OAuth 或业务写入。
只有状态与公开应用元信息可匿名访问，其他路径返回 404。

## 身份与发送边界

骨架不伪造登录，不提供默认管理员，也不以开发假用户绕过学校授权。
identity 与 schools 实现前不开放业务接口；新增浏览器写入必须同时加入会话、CSRF 和 Origin 校验。
H5 不初始化 SDK、不发送消息；后续以人工确认、客户端结果与 unknown 状态实现。

## 数据与迁移

PostgreSQL 为新系统唯一数据库。现有 PHP/MySQL 不在此阶段迁移。
启动时执行嵌入式 SQLx forward-only migration；当前仅创建基础元数据表。
后续多实例或长时间迁移必须在发布流程中控制，不将大规模回填放入启动 migration。
数据库密码采用分字段注入，避免 Compose 拼接 URL 时特殊字符解析错误。

## 前端与接口包

React Router 懒加载 admin/wecom 入口，TanStack Query 管理服务器状态。
Ant Design 仅后台引用。无业务读写接口前 contract 仅提供只读 GET client。
feature/api/index.ts 为前端调用边界；ESLint 限制直接导入 contract 和裸 fetch。
Rust 序列化结果与 TypeScript client 共同核对 api-contract/fixtures 下的固定响应样例。
将来 API 版本、字段和错误码修改必须同步样例与两端测试。

## 部署

基础服务只有 web、api、postgres；开发 overlay 增加本地构建与环回端口。
api 连接 internal 数据库网络及可出站网络，为后续官方 API 留出网络能力。
PostgreSQL 不连接外网网络。web 与 api 根文件系统只读，数据库和 Caddy 数据使用持久卷。
Docker 构建上下文白名单不包含旧项目资源。

## 后果

该交付验证工程运行、部署和测试链路，不是可投入真实校园运营的完整业务版本。
后续身份、学校数据隔离、内容审核与人工分享的业务决策见 [ADR 0002](0002-business-migration.md)。
