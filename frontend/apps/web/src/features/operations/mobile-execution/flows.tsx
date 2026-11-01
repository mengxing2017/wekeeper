import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Card, Space, Table } from 'antd'
import { api } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileExecutionFlows({ school }: { school: string; admin: boolean }) {
  const [page, setPage] = useState(1)
  const rows = useQuery({ queryKey: ['mobile-execution', school, 'assignments', page], queryFn: () => api.assignments(school, page) })
  return <Space orientation="vertical" style={{ width: '100%' }}>
    <Alert type="warning" showIcon title="执行配置正在迁移到共享手机能力管理" description="不再按学校复制设备配置或接受学校设备授权。旧配置只读保留，不可创建验证、发布或启用；新版设备能力由平台资源维护人员管理。此处的历史可用状态不能作为当前发送许可。" />
    {rows.error && <Alert type="error" title={rows.error.message} />}
    <Card title="本校历史配置">
      <Table rowKey="id" pagination={false} dataSource={rows.data?.items ?? []} loading={rows.isFetching} columns={[
        { title: '原手机', dataIndex: 'device_name' }, { title: '原发布', dataIndex: 'release_name' },
        { title: '版本', dataIndex: 'execution_revision' }, { title: '执行资格', render: () => '旧配置已停止受理' },
      ]} />
      <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="本校历史配置" />
    </Card>
  </Space>
}
