# 设备环境观测与控制协议

设备业务路径使用 v10；WSS 控制帧与心跳的 `protocol_version` 为 2。心跳只包含连接身份、运行实例、心跳关联及执行/命令回执，不包含就绪汇总、阻塞原因或手机意愿。版本不匹配拒绝，旧协议不回退。

可选接口 `POST /api/v10/device-execution/environment-observations` 使用设备 Bearer 凭据和 `Idempotency-Key`，JSON:API 类型为 `mobile-device-environments`。请求必须包含下表所有字段，不接受其他字段。

| 字段 | 类型/取值 |
| --- | --- |
| schema | `device_environment/1` |
| observation_id | 非空 UUID |
| observed_at | UTC RFC3339 时间 |
| interactive、keyguard_locked、device_locked、keyguard_secure | `yes` / `no` / `unknown` |
| accessibility、notification | `yes` / `no` / `unknown` |
| wecom_login | `logged_in` / `login_required` / `unknown` |

未知事实显式传 `unknown`。环境观测不用于候选、预览、确认、授权或签发判断，不要求定期上报。设备时间不得超出服务端时间五分钟。平台按当前凭据代次保存最新观测；页面注明观测时间，不能据此宣称当前一定可以执行。共享 fixture 位于 `api-contract/fixtures/device-environment.json`。

`authority-context`、`authorization-decisions`、`idle-telemetry` 已删除。平台不受理或返回手机托管状态；`task-context` 只提供任务摘要、检查目标和验签材料。

设备收到完整签名包后自行处理本地执行条件。未开始的包无法执行时，使用原结果凭据提交 `outcome=not_called`、`reason=device_unavailable`，`consumed=false`、`calling_marked=false`，不要在 evidence 中传入完整本地状态。重复结果使用原 result ID 与相同正文；平台按原尝试结算，继续授权范围内的候选调度。该结果不修改设备全局状态。

诊断仅关联已失败的原 execution，code 限于 `device_unavailable`、`execution_failed`、`target_not_found`、`target_ambiguous`、`execution_timeout`、`protocol_error`；stage 限于 `receive`、`execute`、`report`。details 只接受 app_version、launcher_version、android_version、launcher_package、engine_version 技术版本，不上传本机偏好或状态快照。

控制命令回执的 result_code 可省略，提供时限于 applied、status_reported、execution_cancelled、execution_not_found、already_finished、unsupported、protocol_error，不承载本机状态。
