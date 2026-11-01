import { SchoolFieldChecks } from './field-checks'
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Table, Tag } from 'antd'
import { api, type TargetBinding } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileTargetBindings({ school, admin=false }: { school: string; admin?: boolean }) {
  const [checksOpen,setChecksOpen]=useState(false)
  const [page, setPage] = useState(1)
  const rows = useQuery({ queryKey: ['mobile-execution', school, 'bindings', page], queryFn: () => api.bindings(school, page) })
  return <><Card title="共享手机与目标群绑定">
    <Alert type="info" showIcon title="群属于本校，手机属于共享资源池" description="绑定由平台资源维护人员管理。尚未绑定或需要修改搜索名时，请联系平台配置；此处只显示本校关系。绑定不证明手机已加入真实群，在线也不代表能立即发送。" />
    {rows.error && <Alert type="error" title={rows.error.message} />}
    <Table<TargetBinding> rowKey="id" dataSource={rows.data?.items ?? []} loading={rows.isFetching} pagination={false} columns={[
      { title: '手机', render: (_, row) => row.device_name || row.device_id },
      { title: '有效搜索文字', render: (_, row) => <span style={{ whiteSpace: 'pre-wrap' }}>{row.search_text}</span> },
      { title: '文字来源', render: (_, row) => row.search_source === 'binding_override' ? '该手机覆盖名' : '目标群默认名' },
      { title: '群别名', dataIndex: 'target_alias' },
      { title: '状态', render: (_, row) => <Tag>{!row.enabled ? '绑定已停用' : row.available ? '资源有效，任务仍需确认及调度' : '设备或目标暂不可用'}</Tag> },
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="设备目标绑定" />
  <Button onClick={()=>setChecksOpen(value=>!value)}>{checksOpen ? '收起现场检查' : '查看现场检查邀请与结果'}</Button></Card>{checksOpen && <SchoolFieldChecks school={school} admin={admin} bindings={rows.data?.items ?? []} />}</>
}
