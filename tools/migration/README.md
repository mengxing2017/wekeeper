# 历史论坛迁移参考工具

本目录保留旧 MySQL 快照导出和旧论坛 schema 的离线转换工具，供历史核对；不会读取项目凭据或自动连接业务 PostgreSQL。

当前 WeKeeper 已通过 `202611080140_retire_unused_tables.sql` 删除论坛、学校、导入批次和旧 ID 映射表。这里生成的 `apply.sql`、`verify.sql` 仅适用于退场前的隔离历史 schema，禁止用于升级后的 WeKeeper。转换输出包含旧表检查，以明确拒绝不匹配的目标 schema。

当前论坛迁移由独立 linjian 项目承接。不得批量执行旧 `sql/` 文件，也不得恢复 Java Binlog、Hook 或旧发送进程同步新数据。
