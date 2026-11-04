# 手机共享发送运行配置

本手册供开发者和运维使用。学校用户只填写消息、选择群并预览，然后确认发送，不办理规则上传、发行激活或研发验收。

## 资源与部署

运行配置由实际 `FlowV3`、规范化摘要、不可变 UUID、配置版本和能力组成。引擎 `otter-ui/3`、schema 3、协议 9、企业微信群文字能力以及真实控件、唯一匹配、返回路径、步数、时间与节奏约束都由领域校验器校验。包中不包含 APK、验收记录、72 小时测试记录或审批字段。

开发者将真实核验的 `wecom-group-text.json` 放到 `backend/runtime-resources/` 后构建镜像；Dockerfile 将此目录复制到 `/opt/wekeeper/runtime-resources`。仓库没有附带经过真机验证的正式规则包，不能以 synthetic fixture 代替。不同企业微信/桌面版本需要先完成实际页面适配验证；仅结构合法不代表适配所有客户端。

也可以提供只读目录，避免重建镜像：

```yaml
# compose.runtime.yaml
services:
  api:
    volumes:
      - ./runtime-resources:/opt/wekeeper/runtime-resources:ro
```

在 `.env` 中设置容器内路径：

```dotenv
WEKEEPER_SHARED_SEND_CONFIGURATION=/opt/wekeeper/runtime-resources/wecom-group-text.json
```

`docker compose -f compose.yaml -f compose.runtime.yaml up -d` 启动时先执行数据库 migration，再解析、校验、事务安装配置并切换当前引用。安装相同包可重复执行，不增加业务步骤。不设置路径时先查找镜像内默认包 `/opt/wekeeper/runtime-resources/wecom-group-text.json`，有包则自动安装，否则使用已安装的当前引用；初次部署没有真实包时，此能力不可用，预览拒绝确认，其他功能继续提供服务。

无效配置不覆盖原有效配置。启动日志会报告安装失败；同时应通过以下命令独立检查部署包与当前配置，不能以整站 `/readyz` 代替手机能力验收。HTTP 存活与数据库就绪仍独立于可选手机能力。

## 命令

以下工具和 API 使用相同的 `WEKEEPER_DATABASE_*` 环境变量，不把凭据写入命令行或输出。运行前数据库应已随 API 完成 migration。

```bash
# 无数据库写入，可用于打包阶段。失败返回非零。
mobile-runtime-config validate --package /opt/wekeeper/runtime-resources/wecom-group-text.json

# 事务校验、安装并选为当前配置；无需研发服务账号、证据或额外 activate。
mobile-runtime-config install --package /opt/wekeeper/runtime-resources/wecom-group-text.json

# 只读。配置不存在或校验不通过返回非零。
mobile-runtime-config status
```

Compose 中相应使用 `docker compose exec api mobile-runtime-config ...`。不使用 `mobile-capability import/activate` 来提供业务运行配置；原研发交付工具保留供研发使用，其发布、撤回不会代替或改写当前运行配置。

诊断仅输出能力、是否可用、配置 UUID/版本/摘要、错误代码及位置，不输出 flow、业务正文、APK 或凭据。原因包括 `configuration_missing`、`configuration_structure_invalid`、`configuration_constraints_invalid`、`configuration_digest_mismatch`、`configuration_engine_unsupported`。包身份冲突或文件不可读在安装入口明确失败。先验证文件再执行安装，防止将旧有效配置状态误当作新包已安装。

## 业务与手机状态

预览、readiness 和新自动检查从同一当前引用解析配置，确认时复核依赖。确认后的实际规则冻结到计划，后续替换运行配置不修改已确认消息、原调用身份或 unknown 结果。不读取研发验收表选择规则，也不按最新研发 release 的创建时间猜当前版本。

手机上报独立的屏幕交互、键盘锁、安全锁事实，以及本机自动唤醒同意、屏幕锁是否持有、准备阶段、最近失败和重试时间。安全密码锁等待本人解锁；屏幕关闭与非安全键盘锁可在已同意自动唤醒且有有效待办时尝试准备，后台启动被系统拒绝时显示具体原因。手机托管同意不代表自动唤醒同意。

关闭自动唤醒采用持续托管保屏：已亮屏、托管有效及服务可用时跨应用续持屏幕锁；不会自动解除人工锁屏。开启自动唤醒采用按需模式：空闲可熄屏，有有效待办时准备。暂停、撤回、凭据换代和系统权限限制仍生效，不能用清库、改为静止或释放其他 holder/fence 租约来排队解堵。

## 验证

用隔离数据库验证：无需任何研发表记录即可解析配置并预览/确认；缺包不受理；无效更新不能覆盖原配置；相同 UUID 不同内容被拒；新配置不改写旧计划。数据库测试仅接受 `WEKEEPER_TEST_DATABASE_URL`。

在专用真机验证熄屏唤醒、安全锁等待、解除安全锁后重试、跨应用保屏、后台启动限制、暂停取消及原调用恢复。未连接手机时只能声明编译和模拟检查结果，不能声明真实发送流程已走通。
