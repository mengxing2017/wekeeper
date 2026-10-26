import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Descriptions, Space, Tag, Typography } from 'antd'
import { getReadiness, getSystemInfo } from './api'

const modules = [
  { name: '学校与权限', detail: '学校范围、成员身份与运营角色' },
  { name: '内容与审核', detail: '校园动态、审核记录与内容治理' },
  { name: '模板与分享', detail: '模板版本、内容预览与人工确认' },
  { name: '记录与核对', detail: '发送尝试、异常核对与操作审计' },
]

export function Overview() {
  const system = useQuery({ queryKey: ['system'], queryFn: ({ signal }) => getSystemInfo(signal) })
  const readiness = useQuery({ queryKey: ['readiness'], queryFn: ({ signal }) => getReadiness(signal) })
  const refresh = () => { void system.refetch(); void readiness.refetch() }

  return <Space orientation="vertical" size="large" className="full-width">
    <Alert title="当前为基础骨架" description="已连接应用状态与数据库检查。登录、学校权限和业务操作将在后续阶段开放。" type="info" showIcon />
    <Card title="服务状态" extra={<Button onClick={refresh} loading={system.isFetching || readiness.isFetching}>刷新</Button>}>
      {system.isPending ? <p role="status">正在连接 API…</p> : system.isError ?
        <Alert title="API 暂不可用" description={system.error.message} type="error" showIcon /> :
        <Descriptions column={{ xs: 1, sm: 3 }} items={[
          { key: 'app', label: '应用', children: system.data.application },
          { key: 'version', label: '版本', children: system.data.version },
          { key: 'db', label: '数据库', children: <Tag color={readiness.isSuccess ? 'success' : readiness.isError ? 'error' : 'default'}>{readiness.isSuccess ? '已就绪' : readiness.isError ? '不可用' : '检查中'}</Tag> },
        ]} />}
      {readiness.isError && !system.isError && <Alert className="readiness-alert" title="数据库尚未就绪" description="请检查数据库连接与迁移状态。" type="warning" showIcon />}
    </Card>
    <section aria-label="规划中的业务模块" className="module-grid">
      {modules.map(module => <Card key={module.name} title={module.name} extra={<Tag>待接入</Tag>}>
        <Typography.Paragraph type="secondary">{module.detail}</Typography.Paragraph>
      </Card>)}
    </section>
  </Space>
}
