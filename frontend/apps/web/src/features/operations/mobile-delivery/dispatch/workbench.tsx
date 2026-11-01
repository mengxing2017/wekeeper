import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Card, Table } from 'antd'
import { api } from '../api'
import { PaginationControls } from '../../../../shared/pagination-controls'

/** Mounted only after explicit history navigation. No write clients or nested workbenches. */
export function LegacyMaintenanceHistory() {
  const [page, setPage] = useState(1)
  const flows = useQuery({ queryKey: ['legacy-v5', 'flows', page], queryFn: () => api.dispatch.flows(page) })
  const pacing = useQuery({ queryKey: ['legacy-v5', 'pacing', page], queryFn: () => api.dispatch.pacing(page) })
  const releases = useQuery({ queryKey: ['legacy-v5', 'releases', page], queryFn: () => api.dispatch.releases(undefined, page) })
  return <Card title="历史 v5 资料（只读）">
    <Alert type="warning" title="旧 v5 已停用，不作为当前发送许可" description="不再新建、复制激活、验证、发布或继续发送。历史证据与未决记录保留；取消及撤销可通过受权限控制的治理接口处理。" />
    {[flows.error, releases.error, pacing.error].map((error, i) => error && <Alert key={i} type="error" title={error.message} />)}
    <Table rowKey="id" pagination={false} loading={flows.isFetching} dataSource={flows.data?.items ?? []} columns={[{ title: '历史流程', dataIndex: 'name' }, { title: '摘要', dataIndex: 'content_digest' }]} expandable={{ expandedRowRender: row => <pre>{JSON.stringify(row.body, null, 2)}</pre> }} />
    <Table rowKey="id" pagination={false} loading={releases.isFetching} dataSource={releases.data?.items ?? []} columns={[{ title: '历史发布', dataIndex: 'name' }, { title: '原学校', dataIndex: 'school_id' }, { title: '原验证证据', dataIndex: 'flow_validation_id' }, { title: '原状态', render: (_, row) => row.revoked ? '已撤销' : '历史记录，执行已停用' }]} expandable={{expandedRowRender:row=><HistoricalValidationEvidence school={row.school_id} />}} />
    <Table rowKey="id" pagination={false} dataSource={pacing.data?.items ?? []} columns={[{title:'历史人工基准',dataIndex:'name'},{title:'原学校',dataIndex:'school_id'},{title:'原始样本来源',render:(_,row)=>row.validation_ids.join('、')}]} expandable={{expandedRowRender:row=><pre>{JSON.stringify(row.baseline,null,2)}</pre>}} />
    <PaginationControls page={page} more={flows.data?.page?.has_more || releases.data?.page?.has_more || pacing.data?.page?.has_more} change={setPage} subject="历史 v5 资料" />
  </Card>
}

function HistoricalValidationEvidence({school}:{school:string}) {
  const [page,setPage]=useState(1)
  const rows=useQuery({queryKey:['legacy-v5',school,'validation-evidence',page],queryFn:()=>api.dispatch.validations(school,page)})
  return <>
    {rows.error && <Alert type="error" title={rows.error.message} />}
    <Table rowKey="id" pagination={false} dataSource={rows.data?.items ?? []} columns={[{title:'原验证',dataIndex:'id'},{title:'历史状态',dataIndex:'state'},{title:'原手机',dataIndex:'device_id'}]} expandable={{expandedRowRender:row=><pre>{JSON.stringify(row.result,null,2)}</pre>}} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="历史验证证据" />
  </>
}
