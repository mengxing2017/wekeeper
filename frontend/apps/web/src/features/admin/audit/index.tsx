import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Drawer, Input, Table } from 'antd'
import { auditEvents, type AuditEvent } from './api'
import { ListPanel } from '../../../shared/list-panel'
import { PaginationControls } from '../../../shared/pagination-controls'
import { formatDate } from '../../../shared/format-date'

export function AuditEvents() {
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState<AuditEvent>()
  const query = `?${new URLSearchParams({ 'page[number]': String(page), 'page[size]': '30', ...(search ? { search } : {}) })}`
  const records = useQuery({ queryKey: ['audit-events', query], queryFn: ({ signal }) => auditEvents.list(query, signal) })
  return <>
    {records.error && <Alert type="error" title={records.error.message} />}
    <ListPanel title="审计记录" filters={<><Input.Search aria-label="搜索审计记录" placeholder="搜索操作、执行人、对象或请求标识" onSearch={value => { setSearch(value); setPage(1) }} /><Button onClick={() => void records.refetch()} loading={records.isFetching}>刷新</Button></>} footer={<PaginationControls page={page} more={records.data?.page?.has_more} busy={records.isFetching} change={setPage} />}>
      <Table<AuditEvent> rowKey="id" loading={records.isFetching} dataSource={records.data?.items ?? []} pagination={false} scroll={{ x: 1000 }} columns={[
        { title: '操作', dataIndex: 'action' },
        { title: '执行人', dataIndex: 'actor_id', render: value => value || '系统' },
        { title: '对象', dataIndex: 'object_id', render: value => value || '—' },
        { title: '请求标识', dataIndex: 'request_id', render: value => value || '—' },
        { title: '时间', dataIndex: 'created_at', render: value => formatDate(value) },
        { title: '详情', render: (_, record) => <Button type="link" onClick={() => setSelected(record)}>查看详情</Button> },
      ]} />
    </ListPanel>
    <Drawer title="审计详情" open={!!selected} onClose={() => setSelected(undefined)} size="large">
      {selected && <><p>操作：{selected.action}</p><p>执行人：{selected.actor_id || '系统'}</p><p>对象：{selected.object_id || '—'}</p><p>请求标识：{selected.request_id || '—'}</p><p>时间：{formatDate(selected.created_at)}</p><pre>{JSON.stringify(selected.detail, null, 2)}</pre></>}
    </Drawer>
  </>
}
