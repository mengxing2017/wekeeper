import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Modal, Space, Table } from 'antd'
import { api, type ExecutionTaskDetail } from './api'
import { formatDate } from '../../../shared/format-date'
import { PaginationControls } from '../../../shared/pagination-controls'
import { canCancelTask, taskCounts, taskError, taskItemLabels, taskLabels } from './task-model'

type Props = { school: string; admin: boolean; onNavigate?: (view: string) => void }
export function MobileExecutionTasks(props: Props) {
  return <TaskHistory key={`${props.school}:${props.admin}`} {...props} />
}
function TaskHistory({ school, onNavigate }: Props) {
  const cache = useQueryClient()
  const [page, setPage] = useState(1)
  const [detail, setDetail] = useState<ExecutionTaskDetail>()
  const [cancelTarget, setCancelTarget] = useState<ExecutionTaskDetail>()
  const tasks = useQuery({ queryKey: ['mobile-execution', school, 'tasks', page], queryFn: () => api.tasks(school, page) })
  const cancel = useMutation({ mutationFn: (task: ExecutionTaskDetail) => api.cancelTask(school, task.id, task.revision, '取消旧版尚未领取任务', `mobile-task-cancel:${task.id}:${task.revision}`), onSuccess: () => { setCancelTarget(undefined); setDetail(undefined); void cache.invalidateQueries({ queryKey: ['mobile-execution', school] }) } })
  const denied = [tasks.error, cancel.error].find(e => e && 'code' in e && ['forbidden', 'unauthorized'].includes(String(e.code)))
  if (denied) return <Alert type="error" title={taskError(denied)} description="请重新登录或选择学校；旧学校任务不会继续显示。" />
  return <Space orientation="vertical" style={{ width: '100%' }}>
    <Alert type="warning" showIcon title="共享手机执行升级中，暂不开放新任务确认" description="目标群与共享手机绑定已改为新模型。旧的先选手机、按学校授权及确认即预留流程已停止；新版按群预览与共享调度完成验收后开放。以下保留历史任务及核对依据，不会自动迁移成新发送。企业微信官方客户群群发继续独立使用。" />
    {onNavigate && <Button onClick={() => onNavigate('mobile-targets')}>维护本校手机目标群</Button>}
    {(tasks.error || cancel.error) && <Alert type="error" title={taskError(tasks.error || cancel.error)} />}
    <Card title="历史手机任务">
      <Table<ExecutionTaskDetail> rowKey="id" dataSource={tasks.data?.items ?? []} loading={tasks.isFetching} pagination={false} columns={[
        { title: '登记时间', render: (_, t) => formatDate(t.created_at) },
        { title: '原计划手机', render: (_, t) => t.device_name || t.device_id },
        { title: '状态', render: (_, t) => taskLabels[t.state] },
        { title: '逐群进度', render: (_, t) => taskCounts(t) },
        { title: '操作', render: (_, t) => <Space><Button onClick={() => setDetail(t)}>查看原计划</Button>{canCancelTask(t) && <Button danger onClick={() => setCancelTarget(t)}>取消未领取任务</Button>}</Space> },
      ]} />
      <PaginationControls page={page} more={tasks.data?.page?.has_more} change={setPage} subject="历史手机任务" />
    </Card>
    <Modal title="原计划与逐群记录" open={!!detail} footer={null} onCancel={() => setDetail(undefined)}>
      {detail && <><p>原计划保留，不会因共享绑定变更而改写。</p><pre style={{ whiteSpace: 'pre-wrap' }}>{detail.plan.message}</pre><Table rowKey="id" pagination={false} dataSource={detail.items} columns={[
        { title: '原搜索文字', render: (_, i) => <span style={{ whiteSpace: 'pre-wrap' }}>{i.snapshot.search_text}</span> },
        { title: '状态', render: (_, i) => taskItemLabels[i.state] },
      ]} /></>}
    </Modal>
    <Modal title="取消未领取任务？" open={!!cancelTarget} confirmLoading={cancel.isPending} onCancel={() => setCancelTarget(undefined)} onOk={() => cancelTarget && cancel.mutate(cancelTarget)}>
      只取消尚未领取的旧任务。已调用或待核对项目不能通过此操作解除风险状态。
    </Modal>
  </Space>
}
