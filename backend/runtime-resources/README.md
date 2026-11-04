# 手机共享发送运行资源

此目录用于随部署镜像携带真实、经过页面核验的执行配置，镜像中路径为 `/opt/wekeeper/runtime-resources`。也可通过 Compose 的只读卷提供同一目录。

正式包文件名为 `wecom-group-text.json`，字段为 `id`（非空 UUID）、`version`、`capability`（`wecom.group_text`）、`flow`（当前 otter-ui/3、协议 9 的实际 FlowV3）和 `flow_digest`（对 flow 递归键排序的规范化 JSON 计算 SHA-256）。版本和 UUID 标识不可变内容；相同 UUID 不能替换不同内容。

目录当前没有真实手机采集和验证的正式规则包。不得把 `api-contract/fixtures` 的合成控件、假证据或测试配置复制为正式包，也不得为空配置生成假规则。开发者需要按实际企业微信版本、桌面及目标页面采集唯一可见控件，完成无发送检查后形成该产品资源；测试与验证记录保存在研发流程中，不进入包的业务门禁。

部署、校验、安装和只读诊断见 `docs/runbooks/mobile-runtime-configuration.md`。缺包时只有此能力明确不可用，其他独立功能继续运行；不得向学校要求办理研发交付手续。
