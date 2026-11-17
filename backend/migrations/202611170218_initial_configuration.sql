-- 空库默认配置。只创建当前业务必需的配置，不生成旧工作空间、历史规则或重查任务。
INSERT INTO application_metadata(singleton,schema_generation) VALUES(true,1);
INSERT INTO wecom_asset_sync_state(dataset) VALUES('groups');
INSERT INTO device_control_policy(singleton) VALUES(true);
INSERT INTO mobile_shared_platform_identity(singleton,id) VALUES(true,gen_random_uuid());
INSERT INTO distribution_sources(namespace) VALUES('builtin.direct'),('builtin.source-route');
INSERT INTO distribution_source_turns(source_namespace) VALUES('builtin.direct');
SELECT keyword_scope_init('global','global','global',NULL,'全局规则');
