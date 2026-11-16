# 来源消息接入 SDK 文档

本文面向向 WeKeeper 提交消息的外部服务。使用签名 HTTP 接口即可接入，不依赖 WeKeeper 内部项目或浏览器登录。所有请求由调用方主动发起；WeKeeper 不回调调用方，也不查询调用方在线状态。

## 连接配置

由 WeKeeper 系统管理员提供服务地址 `origin`、凭据 ID `key_id`、签名密钥 `secret` 和已授权的 `route_id`。密钥只保存在调用方服务端。`secret` 使用无填充的 Base64URL 编码，签名前必须解码为原始字节。

消息正文由调用方完整构造。WeKeeper 不拼接标题、详情链接，不校验正文中的 HTTP、HTTPS 或小程序链接，不要求登记可信域名。连接 WeKeeper 的服务地址与正文中的链接是不同用途。

## 接口清单

| 方法 | 路径 | 用途 | 成功状态 |
| --- | --- | --- | --- |
| POST | `/api/v2/source-events` | 提交普通消息 | 新建接收为 202；相同请求重投为 200 |
| GET | `/api/v2/source-receipts/{receipt_id}` | 核对接收回执 | 200 |
| POST | `/api/v2/source-authentications` | 检查凭据并查询已授权路由 | 200 |
| POST | `/api/v2/source-route-checks` | 检查路由接入授权 | 200 |

认证与路由检查用于连接诊断，无需在每条消息提交前调用。它们只表示接入授权，不表示内部发送配置或真实送达状态。

## 请求签名

所有接口均使用以下请求头。每个请求头只发送一次，不使用 Cookie 或 Bearer 身份替代签名。

| 请求头 | 值 |
| --- | --- |
| `Accept` | `application/vnd.api+json` |
| `Content-Type` | POST 必须为 `application/vnd.api+json`，不附加 charset |
| `X-Source-Version` | `2` |
| `X-Source-Key-Id` | 分配的 `key_id` |
| `X-Source-Timestamp` | 当前 Unix 秒时间戳，十进制整数；与服务器时间相差不超过 300 秒 |
| `X-Source-Nonce` | 每次请求新生成的小写标准 UUID |
| `Idempotency-Key` | 提交消息时为 `event_id`；查询回执时仍为原 `event_id`；检查接口为本次检查生成的 UUID |
| `X-Source-Signature` | HMAC-SHA256 签名，64 位小写十六进制 |
| `X-Request-Id` | 可选，调用方的请求跟踪标识 |

路径不带查询串、末尾斜杠、重复斜杠或百分号编码。签名路径只包含 `/api/v2/...`，不包含协议、域名或端口。GET 回执请求体必须为空。

先对实际发送的请求体原始字节计算 SHA-256，输出小写十六进制。以下八行以单个 LF（`\n`）连接，最后一行末尾不加换行，然后使用解码后的 secret 对整个 UTF-8 签名串计算 HMAC-SHA256：

```text
2
{key_id}
{大写 HTTP 方法}
{请求路径}
{X-Source-Timestamp}
{X-Source-Nonce}
{Idempotency-Key}
{请求体 SHA-256 小写十六进制}
```

Python 签名示例（仅使用标准库）：

```python
import base64
import hashlib
import hmac
import time
import uuid


def signed_headers(key_id, secret, method, path, identity, body):
    timestamp = str(int(time.time()))
    nonce = str(uuid.uuid4())
    digest = hashlib.sha256(body).hexdigest()
    canonical = "\n".join([
        "2", key_id, method, path, timestamp, nonce, identity, digest
    ])
    key = base64.urlsafe_b64decode(secret + "=" * (-len(secret) % 4))
    signature = hmac.new(key, canonical.encode("utf-8"), hashlib.sha256).hexdigest()
    headers = {
        "Accept": "application/vnd.api+json",
        "X-Source-Version": "2",
        "X-Source-Key-Id": key_id,
        "X-Source-Timestamp": timestamp,
        "X-Source-Nonce": nonce,
        "Idempotency-Key": identity,
        "X-Source-Signature": signature,
    }
    if method == "POST":
        headers["Content-Type"] = "application/vnd.api+json"
    return headers
```

`body` 必须是之后直接发送的 `bytes`。JSON 只序列化一次，签名后不要调整缩进、字段顺序或换行。GET 使用 `body=b""`。每次重试重新生成时间戳、nonce 和签名。

## 提交消息

`POST /api/v2/source-events`

```json
{
  "data": {
    "type": "source-events",
    "id": "00000000-0000-0000-0000-000000000001",
    "attributes": {
      "event_id": "00000000-0000-0000-0000-000000000001",
      "schema_version": 2,
      "route_id": "assigned-route",
      "event_type": "content.upsert",
      "occurred_at": "2026-11-16T00:00:00Z",
      "content": "活动通知\n明天下午三点开始。\n#小程序://Linjian/示例"
    }
  }
}
```

| 字段 | 类型与要求 |
| --- | --- |
| `data.type` | 固定为 `source-events` |
| `data.id` | UUID，与 `event_id` 和 `Idempotency-Key` 相同 |
| `event_id` | 调用方生成的消息唯一 UUID |
| `schema_version` | 整数，固定为 `2` |
| `route_id` | 已分配且授权的路由标识；1–128 个 ASCII 字母、数字、下划线或短横线 |
| `event_type` | 固定为 `content.upsert`，仅代表提交一条消息，不更新或覆盖旧消息 |
| `occurred_at` | 带时区的 RFC 3339 时间，仅供记录消息发生时间 |
| `content` | 调用方组织好的完整纯文本，必填、非 null、非空且不能全为空白；最多 200 个 Unicode 标量 |

所有字段均必填，不接受额外字段。`title`、`detail_url`、`external_content_id`、`version`、`sequence` 均不属于此接口；需要发送的标题和链接直接写入 `content`。WeKeeper 保留正文原始空格和换行。

正文中的汉字、英文、标点、空格、换行及 emoji 均参与长度计算；组合 emoji 可能包含多个 Unicode 标量。JavaScript 可使用 `Array.from(content).length` 计数，不使用 UTF-16 的 `content.length`。200 个字符可接收，201 个字符被拒绝，不自动截断。总请求体上限为 1 MiB。

不接收撤回通知，不执行消息撤回或旧消息替换。`content.withdrawn` 及其他事件类型返回 `400 unsupported_event_type`，不建立接收回执。

## 接收响应与回执查询

新消息完成持久接收后立即返回 HTTP 202：

```json
{
  "data": {
    "type": "source-receipts",
    "id": "00000000-0000-0000-0000-000000000002",
    "attributes": {
      "event_id": "00000000-0000-0000-0000-000000000001",
      "status": "received",
      "received_at": "2026-11-16T00:00:01Z"
    }
  }
}
```

`data.id` 是 WeKeeper 分配的 `receipt_id`，与调用方 `event_id` 不同。收到 202 或重投时的 200，即可结束本条消息投递。`status` 固定为 `received`，仅表示已接收；不表示发送成功，不等待内部检查或发送。内部执行暂停、敏感词拦截、缺少目标、设备离线或发送失败均不改变已接收事实，不要求调用方重投。

已获得回执 ID 时，可调用 `GET /api/v2/source-receipts/{receipt_id}` 核对，`Idempotency-Key` 使用原 `event_id`。响应结构与上例相同，仅提供接收状态，不提供内部检查或发送状态，不需要持续轮询。

若提交响应丢失，使用原 `event_id` 和完全相同的请求体原始字节重新提交，更新签名时间和 nonce。已接收的相同请求返回 HTTP 200 和原回执，不重复创建消息；相同事件 ID 携带不同的合法正文返回 `409 event_conflict`。不要为了重试生成新事件 ID。

## 凭据与路由检查

两个检查接口的请求结构相同，`data.type` 分别为 `source-authentications` 和 `source-route-checks`。`data.id` 与本次请求的 `Idempotency-Key` 相同。例如：

```json
{
  "data": {
    "type": "source-route-checks",
    "id": "00000000-0000-0000-0000-000000000003",
    "attributes": {
      "route_id": "assigned-route"
    }
  }
}
```

`attributes` 可以为空对象。可选字段为 `route_id`、`limit`（1–100，默认 50）和 `cursor`；分页时传入上次响应的 `next_cursor`，直到为 null。游标失效返回 `409 cursor_stale`，清除游标重新查询。

成功响应的 `data.type` 与请求相同，`data.id` 为本次请求 ID。`attributes` 包含 `source_id`、`signature_versions`、`schema_versions`、`credential_revision`、`authorization_revision`、`paths`、`routes` 和 `next_cursor`。每条路由包含 `route_id`、`available` 和可选的 `display_name`。`available` 仅表示路由接入授权，不包含内部执行开关。指定未授权路由时，可返回该路由 `available: false`；不要据此推断内部发送配置。

## 错误响应与重试

接口使用实际 HTTP 错误状态，不将失败包装为 HTTP 200。错误响应媒体类型为 `application/vnd.api+json`。正文超限示例：

```json
{
  "errors": [{
    "status": "400",
    "code": "content_too_long",
    "title": "消息正文不能超过 200 个字符（汉字、标点、空格和换行均计入）",
    "source": {"pointer": "/data/attributes/content"},
    "meta": {
      "retryable": false,
      "supported_versions": [2],
      "request_id": "example-request"
    }
  }]
}
```

| HTTP 状态 | 错误码 | 含义与处理 |
| --- | --- | --- |
| 400 | `content_too_long` | 正文超过 200 个字符；修改正文，不原样重试 |
| 400 | `content_empty` | 提示“消息正文不能为空或全为空白”；补充正文 |
| 400 | `forbidden_field` | 缺少必填字段、null、类型错误或存在额外字段；核对请求结构 |
| 400 | `unsupported_event_type` | 仅支持普通消息；不接受撤回通知 |
| 400 | `unsupported_version` | 签名版本或消息协议版本不支持；使用版本 2 |
| 400 | `event_identity_mismatch` | 核对资源类型、`data.id`、`event_id` 和幂等请求头 |
| 400 | `invalid_request` | JSON、请求头、路径或其他参数格式不合法 |
| 401 | `authentication_failed` / `credential_unavailable` | 核对签名、密钥及凭据有效期和状态 |
| 401 | `timestamp_expired` | 校准时钟，使用当前时间、新 nonce 重新签名 |
| 403 | `source_disabled` / `route_unavailable` | 调用方停用或缺少路由授权，联系系统管理员 |
| 404 | `receipt_not_found` | 回执不存在或当前凭据无权查询；核对回执 ID 与路由授权 |
| 409 | `event_conflict` | 同一事件 ID 使用了不同正文；核对本地原始请求记录 |
| 409 | `replay_detected` | nonce 已使用；保留业务身份和正文，换新 nonce 重新签名 |
| 409 | `cursor_stale` | 查询游标失效；清除游标重新查询 |
| 413 | `payload_too_large` | 总请求体超过 1 MiB；缩小请求 |
| 415 | `unsupported_media_type` | 核对 Accept、Content-Type；不发送 Content-Encoding |
| 429 | `rate_limited` | 按 `Retry-After` 等待后重试 |
| 503 | `temporarily_unavailable` | 接入暂不可用；结合 `meta.retryable` 与提示处理 |

正文超限和空白错误均定位 `/data/attributes/content`，`meta.retryable` 为 false，不生成接收回执，不进入内部发送流程。错误校验有先后顺序，例如签名错误时会先返回鉴权错误。

SDK 应依据 `code` 分支处理，并展示 `title`，不要把所有错误统一改写为 `invalid_request`。仅在 `meta.retryable` 为 true 时安排自动重试，并遵守 `Retry-After`；503 若 `retryable` 为 false，应联系系统管理员处理配置。遇到网络超时或无法确认是否接收的响应，按原事件身份和原始正文重投，避免产生新消息。
