import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Input, InputNumber, Space, Switch, Table } from 'antd'
import { distributionPolicies, type SourcePolicy, type ResourcePolicy } from './api'

export function DistributionPolicyPanel() {
  const query = useQuery({ queryKey: ['wecom', 'platform', 'policies'], queryFn: distributionPolicies.get })
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [source, setSource] = useState<SourcePolicy>()
  const [resource, setResource] = useState<ResourcePolicy>()
  const save = async () => {
    setBusy(true); setError('')
    try {
      if (source) await distributionPolicies.source(source)
      else if (resource) await distributionPolicies.resource(resource)
      setSource(undefined); setResource(undefined); await query.refetch()
    } catch (e) { setError(e instanceof Error ? e.message : '政策更新失败') }
    finally { setBusy(false) }
  }
  return <Card title="分发资源治理（平台权限）">
    <p>来源积压按来源累计，官方资源调用量跨来源和学校累计。这里的额度是平台运营限制，不代表官方安全发送次数；手机发送仍保持禁用。</p>
    {(error || query.error) && <Alert type="error" title={error || query.error?.message} />}
    <Table<SourcePolicy> rowKey="namespace" pagination={false} dataSource={query.data?.sources ?? []} columns={[
      { title: '受信来源', dataIndex: 'namespace' }, { title: '草稿/待执行限制', dataIndex: 'max_pending' },
      { title: '状态', render: (_, row) => row.enabled ? '允许受理' : '已暂停新发送' },
      { title: '操作', render: (_, row) => <Button onClick={() => { setSource({ ...row }); setResource(undefined) }}>调整来源政策</Button> },
    ]} />
    <Table<ResourcePolicy> rowKey="resource_key" pagination={false} dataSource={query.data?.resources ?? []} columns={[
      { title: '官方真实资源键', dataIndex: 'resource_key' }, { title: '窗口秒数', dataIndex: 'window_seconds' }, { title: '最大调用数', dataIndex: 'max_calls' },
      { title: '状态', render: (_, row) => row.enabled ? '启用' : '暂停' },
      { title: '操作', render: (_, row) => <Button onClick={() => { setResource({ ...row }); setSource(undefined) }}>调整资源政策</Button> },
    ]} />
    <Button onClick={() => { setSource(undefined); setResource({ resource_key: '', window_seconds: 3600, max_calls: 1, enabled: true, revision: 0 }) }}>配置官方资源限制</Button>
    {(source || resource) && <Space orientation="vertical">
      {source && <><p>{source.namespace}</p><label>来源积压限制<InputNumber aria-label="来源积压限制" min={1} max={100000} value={source.max_pending} onChange={v => setSource({ ...source, max_pending: Number(v) })} /></label><Switch aria-label="来源允许新发送" checked={source.enabled} onChange={enabled => setSource({ ...source, enabled })} /></>}
      {resource && <><label>官方真实资源键<Input aria-label="官方真实资源键" placeholder="wecom:官方企业ID:官方成员userid小写" disabled={resource.revision > 0} value={resource.resource_key} onChange={e => setResource({ ...resource, resource_key: e.target.value })} /></label><label>窗口秒数<InputNumber aria-label="窗口秒数" min={1} max={2592000} value={resource.window_seconds} onChange={v => setResource({ ...resource, window_seconds: Number(v) })} /></label><label>窗口调用数<InputNumber aria-label="窗口调用数" min={1} max={100000} value={resource.max_calls} onChange={v => setResource({ ...resource, max_calls: Number(v) })} /></label><Switch aria-label="资源允许新发送" checked={resource.enabled} onChange={enabled => setResource({ ...resource, enabled })} /></>}
      <Space><Button type="primary" loading={busy} onClick={() => void save()}>保存政策</Button><Button onClick={() => { setSource(undefined); setResource(undefined) }}>取消调整</Button></Space>
    </Space>}
  </Card>
}
