import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Card, Table } from 'antd'
import { Link } from 'react-router-dom'
import { api } from './api'
import { ControlStatus, useControlFreshness } from './control-status'
import { PaginationControls } from '../../../shared/pagination-controls'

export function SchoolExecutionResources({ school, accounts, admin }: { school: string; accounts: boolean; admin: boolean }) {
  const [page, setPage] = useState(1)
  const query = useQuery({ queryKey: ['mobile', school, 'execution-resources', page], queryFn: () => api.dispatch.resources(school, page), refetchInterval: 5000, refetchOnWindowFocus: 'always' })
  const fresh = useControlFreshness(query.dataUpdatedAt, query.isError)
  return <Card title={accounts ? '本校可用发送账号' : '本校可用手机设备'}>
    <p>仅展示明确分配给本校、绑定仍有效的执行资源。此处不提供全平台登记、凭据或跨校授权操作。</p>
    {admin && <p><Link to={`/admin?school=${encodeURIComponent(school)}&view=mobile-delivery&subview=applications`}>配置本校执行关系</Link></p>}
    {query.error && <Alert type="error" title={query.error.message} />}
    <Table rowKey="id" loading={query.isFetching} dataSource={query.data?.items ?? []} pagination={false}
      locale={{ emptyText: '暂无已分配资源，请联系学校管理员配置执行关系；全平台资源登记由维护人员处理。' }}
      columns={[
        ...(accounts ? [{ title: '发送账号', dataIndex: 'account_name' }, { title: '企业', dataIndex: 'company_name' }] : [{ title: '手机设备', dataIndex: 'device_name' }]),
        { title: accounts ? '使用设备' : '绑定发送账号', dataIndex: accounts ? 'device_name' : 'account_name' },
        { title: '控制连接', render: (_, row) => <ControlStatus state={row.control_state} fresh={fresh} /> },
        { title: '最近控制心跳', render: (_, row) => row.last_control_heartbeat_at ?? '尚未收到心跳' },
        { title: '资源配置', render: (_, row) => row.available ? '已配置，执行仍需业务授权和任务确认' : ({ device_disabled: '设备已停用', account_disabled: '账号已停用', configuration_unavailable: '流程发布不可用' }[row.block_reason ?? ''] ?? '配置不可用') },
      ]} />
    <PaginationControls page={page} more={query.data?.page?.has_more} change={setPage} subject={accounts ? '本校发送账号' : '本校设备'} />
  </Card>
}
