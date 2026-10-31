import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Space, Table } from 'antd'
import { api } from '../api'
import { PaginationControls } from '../../../../shared/pagination-controls'
const states: Record<string, string> = {
  checking_environment: '检查手机环境',
  returning_to_launcher: '返回安卓桌面',
  matching_launcher: '匹配桌面方案',
  searching_application: '搜索本地应用',
  browsing_all_applications: '查找全部应用',
  opening_application: '打开应用',
  verifying_application: '核验应用',
  ready: '应用已准备',
  draft: '草稿',
  confirmed: '等待手机 / 执行中',
  completed: '已结束，请核对逐群结果',
  stopped: '已停止',
  queued: '等待手机',
  claimed: '搜索群与核验',
  prepared: '准备发送',
  permit_issued: '发送观察',
  ui_observed_sent: '界面观察到发送（非送达证明）',
  unknown: '待人工核对',
  manually_sent: '人工核对已发送',
  verified_not_sent: '人工核对未发送',
  cancelled: '已取消',
}
export function AutomationRuns({
  school,
  openTask,
}: {
  school: string
  openTask: (id: string) => void
}) {
  const [page, setPage] = useState(1),
    [selected, setSelected] = useState('')
  const runs = useQuery({
    queryKey: ['mobile', school, 'automation-runs', page],
    queryFn: () => api.automation.runs(school, page),
  })
  const progress = useQuery({
    queryKey: ['mobile', school, 'automation-run', selected],
    queryFn: () => api.automation.run(school, selected),
    enabled: !!selected,
    refetchInterval: selected ? 10000 : false,
  })
  return (
    <Space orientation="vertical" className="full-width">
      {[runs.error, progress.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      <Table
        rowKey="id"
        pagination={false}
        dataSource={runs.data?.items ?? []}
        columns={[
          { title: '业务', dataIndex: 'name' },
          { title: '状态', render: (_, r) => states[r.task_state ?? ''] ?? '等待核对' },
          {
            title: '操作',
            render: (_, r) => (
              <Space>
                <Button onClick={() => setSelected(r.id)}>逐群进度</Button>
                <Button onClick={() => openTask(r.task_id)}>任务与人工核对</Button>
              </Space>
            ),
          },
        ]}
      />
      <PaginationControls
        page={page}
        more={runs.data?.page?.has_more}
        change={setPage}
        subject="业务执行"
      />
      {progress.data && (
        <Card title="当前群与等待">
          {progress.data.preparation && (
            <p>手机阶段：{states[progress.data.preparation.stage] ?? '准备应用'}</p>
          )}
          <p>
            当前第 {progress.data.current_position} 群。
            {progress.data.pause_reason
              ? '已暂停，请进入任务核对。'
              : progress.data.next_eligible_at
                ? `下群最早可开始时间：${new Date(progress.data.next_eligible_at).toLocaleString()}`
                : '等待手机启动有效会话并准备应用。'}
          </p>
          <Table
            rowKey="id"
            pagination={false}
            dataSource={progress.data.items}
            columns={[
              { title: '顺序', dataIndex: 'position' },
              { title: '结果', render: (_, i) => states[i.state] ?? i.state },
            ]}
          />
        </Card>
      )}
    </Space>
  )
}
