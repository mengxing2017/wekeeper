import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Drawer, Table } from 'antd'
import { api } from '../api'
import { PaginationControls } from '../../../../shared/pagination-controls'

export function DispatchWorkspace({ school, view, openTask }: { school: string; view: 'devices' | 'compose' | 'batches'; openTask: (id: string) => void }) {
  const [page, setPage] = useState(1)
  const [selected, setSelected] = useState('')
  const detail = useQuery({queryKey:['legacy-v5',school,'batch',selected],queryFn:()=>api.dispatch.batch(school,selected),enabled:!!selected})
  const assignments = useQuery({ queryKey: ['legacy-v5', school, 'assignments', page], queryFn: () => api.dispatch.assignments(school, page), enabled: view === 'devices' })
  const batches = useQuery({ queryKey: ['legacy-v5', school, 'batches', page], queryFn: () => api.dispatch.batches(school, page), enabled: view === 'batches' })
  return <Card title="本校历史 v5 资料（只读）">
    <Alert type="warning" title="旧 v5 已停用，不作为当前发送许可" description="不再分配、启用、创建预览或继续发送。请在共享手机业务维护目标群与任务；共享执行链路尚未完成。已有结果和 unknown 核对记录保留。" />
    {[assignments.error, batches.error].map((error, i) => error && <Alert key={i} type="error" title={error.message} />)}
    {view === 'devices' && <Table rowKey="id" pagination={false} dataSource={assignments.data?.items ?? []} columns={[{ title: '原手机', dataIndex: 'device_name' }, { title: '原流程', dataIndex: 'flow_name' }, { title: '执行资格', render: () => '旧配置已停止受理' }]} />}
    {view === 'batches' && <Table rowKey="id" pagination={false} dataSource={batches.data?.items ?? []} columns={[{ title: '历史批次', dataIndex: 'name' }, { title: '历史状态', dataIndex: 'state' }, {title:'历史结果',render:(_,row)=><Button onClick={()=>setSelected(row.id)}>查看批次结果</Button>}]}  />}
    <Drawer open={!!selected} title="历史批次结果（不继续发送）" onClose={()=>setSelected('')} size="large">
      {detail.error && <Alert type="error" title={detail.error.message} />}
      {detail.data?.plans?.map(plan=><Card key={plan.id} title={plan.device_name ?? plan.id}>
        <p>原计划摘要：{plan.content_digest}</p>
        {plan.items?.map(item=><div key={item.id}><p>{item.state} · {item.content}</p><Button onClick={()=>openTask(item.task_id)}>查看原任务与未决核对</Button></div>)}
      </Card>)}
    </Drawer>
    {view !== 'compose'  && <PaginationControls page={page} more={view === 'devices' ? assignments.data?.page?.has_more : batches.data?.page?.has_more} change={setPage} subject="本校历史 v5 资料" />}
  </Card>
}
