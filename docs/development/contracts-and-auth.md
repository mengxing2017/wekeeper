# 接口契约、身份与学校授权

适合同时修改后端和客户端的开发者。共享 DTO/client 只定义在 `api-contract/`，业务 JSON:API 与 Rust 序列化使用共享 fixture 核对。

## 一次接口修改的步骤

1. 明确资源类型、字段、状态、错误码及授权范围。
2. 修改 Rust 请求/响应模型、用例和端口实现；HTTP 层不写 SQL。
3. 修改 `api-contract/` 对应模块类型与 client，同步 fixture。
4. 仅在对应 feature 的 `api/index.ts` 引用 contract，由页面使用该入口；不在 shared 重写 DTO 或绕过 client 请求。
5. 同时执行 TypeScript 契约测试与 Rust 序列化契约测试。TS 编译不能证明 Rust wire 格式一致。

## 凭据与边界

| 调用者 | 凭据与检查 |
| --- | --- |
| Web 后台 | Cookie session、Origin、CSRF 及用例权限 |
| 小程序 | 独立 Bearer transport，不使用浏览器 Cookie |
| 手机设备 | 专用设备凭据与代次/会话，不接受浏览器身份冒充 |
| 公开阅读 | 仅学校显式启用的范围，不能默认全站公开 |

平台管理员与学校授权分开检查。模板、群、设备和任务要核对所属学校及当前有效状态，不能只因为前端可见就跳过后端授权。

## 发送接口

创建、确认、领取和结果上报必须保持当前幂等及 fencing 语义。人工确认、单次调用领取、unknown 人工核对不因新增通道而放宽。不接入千寻、Hook 或 Binlog 发送。

设备 v1—v5 的恢复与在途记录保持原协议，不能强行升级旧请求。最新流程见[当前 v5](../runbooks/dispatch-v5.md)，历史解释见[参考资料](../references/index.md)。
