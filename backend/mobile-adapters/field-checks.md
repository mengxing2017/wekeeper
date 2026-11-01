# 共享手机无发送检查接口（服务端增量）

已接入学校邀请页面、平台发起页面和 keeper-otter 原生无发送检查 IPC、固定日志与补报。代码检查不等于生产适配和真机验收：当前仍没有生产适配发行，任务发送保持关闭。不得使用本接口发放发送许可，或将 `passed` 转换成能力 enabled。

## 授权和调用顺序

1. 学校管理员对本校现有且有效的设备目标绑定发出邀请：
   `POST /api/v1/schools/{school}/mobile-execution/field-check-invitations`，类型 `mobile-field-check-invitations`。
   仅接收 `binding_id`、`expected_binding_revision`、`expected_target_locator_revision`。
   搜索文字、学校和设备从绑定取得，不接受平台自报目标文字或旧学校设备 grant。
2. 平台管理员引用邀请和有效开通申请：
   `POST /api/v1/platform/mobile-execution/field-checks`，类型 `mobile-field-checks`。
   仅接收 `opening_id`、`invitation_id`、`expected_invitation_revision`。
   同一设备已有相同待办时返回原记录；另一待办冲突。邀请只产生一个检查，失败、过期后须由学校重新邀请。
3. 手机使用设备专用 bearer 读取 `GET /api/v7/device-execution/field-checks`，类型 `mobile-field-check-contexts`，资源内 `jobs` 为有效待办，仅当前设备、当前凭据代次。浏览器 cookie 不作为设备凭据。
4. **手机先经本机原生确认并持久登记，再领取**：
   `POST /api/v7/device-execution/field-checks/{id}/claims`，类型 `mobile-field-check-claims`，输入 `expected_revision`。
   服务端检查绑定、目标、邀请发起人当前学校管理员身份、开通、发行、当前环境、配置、同意与凭据代次。
   占用屏障包括历史设备 reservation、旧执行活跃 claim／unknown，以及共享设备未到期 lease。
5. 返回单目标搜索文字、固定 flow／摘要／环境／同意依赖、challenge、单调 lease_fence、服务器时间和期限。
   租约最多 240 秒，且不超过检查申请的 10 分钟截止时间。没有续租或自动重跑接口。
6. 补报 `POST /api/v7/device-execution/field-checks/{id}/results`，类型 `mobile-field-check-results`。
   仅接收 challenge、lease_fence、八项布尔 checks 和有限格式的 evidence 代码，不接收消息正文、聊天记录、客户端自评 passed 或新的目标文字。

学校可 GET 本校 invitations／checks；平台可 GET 平台 invitations／checks。学校对本校邀请执行 `POST .../field-check-invitations/{id}/revocations`，输入 expected_revision。撤销使后续领取失效，但不强制释放仍可能在运行的租约。

所有 POST 均要求稳定 `Idempotency-Key`。管理写入沿用操作审计与固定响应；设备写入按设备、凭据代次、操作和检查 ID 隔离固定响应。不同载荷复用相同键冲突。

## 已实施的手机执行约束

- 必须使用专门的 v7 无发送执行器入口，不复用学校 grant 或旧 v6 启动入口。不得暴露消息输入／发送执行器。
- 领取响应的幂等重放可能已过期。应根据服务器剩余期限扣除请求耗时，并转换为本机单调时钟截止时间；到期、时间异常、重启、停止、锁屏、本机阻止或环境改变均不得操作 UI。
- 领取前后建立持久日志。网络不明、进程终止、重启和断线恢复只允许固定失败证据补报，不得领取后再次执行相同检查。结果键和完整结果必须在请求前持久化。
- 每个 UI 操作前核对本机状态和截止时间；不得在未连接或无法确认依赖时靠历史 passed 放行。
- 所有共享发送入口最终必须使用同一设备仲裁表及事务锁。当前发送关闭，没有宣称历史发送器已迁移。

## 结果解释

服务端根据八项 checks 全部为 true 判定 passed；有效依赖下任一 false 为 failed。租约到期、换代或任何依赖改变，迟到结果只登记 stale。已 expired 的已领取检查仍可登记原许可结果；不会复活检查或授权。

固定结果不可修改或覆盖。释放仅匹配原 holder 和 fence，旧结果不能解锁后续租约。租约到期不证明任何消息已发送或未发送；本接口仅允许无发送检查，unknown 仍由原业务核对闭环处理。

测试 fixture 仅用于临时库和 Rust／TypeScript 契约验证，不得进入生产发行目录。SQL 单用户检查不等价于多连接并发或真实手机验收。


## 当前页面与核对

学校“手机目标群 → 共享手机与目标群绑定 → 查看现场检查邀请与结果”按需加载本校检查。只有本校管理员看到邀请、撤销按钮，邀请使用固定的绑定版本和目标定位版本，不提供任意搜索文字输入。

平台“业务能力开通 → 查看进度与下一步 → 现场无发送检查”仅引用学校已发出的有效邀请。选择设备后查询范围固定到该设备；申请结果不明时保留原负载和键重试。

手机“本机业务能力 → 不发送消息的现场检查”刷新后显式确认开始。整个检查持有本机配置互斥；与本机其他配置/执行不能并发。用户触碰、中断、停止、锁屏、无障碍断开、期限或环境变化均停止。字段日志纳入原平台/设备和凭据换代屏障；恢复入口只补报原领取与固定证据，不调用 UI 执行器。

检查列表每次核验 `dependencies_current`；邀请返回 `available`、`consumed` 与学校名称。开通进度仅在最新检查 passed 且依赖仍有效时移除待检查提示，`execution_ready` 始终为 false。历史通过不代表当前可用、真实群身份或消息送达。
