import {OfficialChannelControl} from './channel-control'
import {BusinessRequests} from './business-requests'
import {OfficialUsageManager} from './usage-manager'
import { DistributionPolicyPanel } from './policies'
import { useState } from 'react'
import { Alert, Button, Card, Space, Table, Tag } from 'antd'
import { useQuery } from '@tanstack/react-query'
import { officialSenders, type OfficialSender } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { useDialogs } from '../../../shared/use-dialogs'

export function OfficialSenders({ school, global }: { school?: string; global: boolean }) {
  const [page, setPage] = useState(1)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const dialogs = useDialogs()
  const query = useQuery({ queryKey: ['wecom', global ? 'platform' : school, 'senders', page], queryFn: () => officialSenders.list(global ? undefined : school, page), enabled: global || !!school })
  const qualify = async (row: OfficialSender) => {
    if (!await dialogs.confirm(`${row.enabled ? '停用' : '启用'}该官方成员在此学校的发送资格？这不会修改历史任务结果，也不赋予手机身份。`)) return
    setBusy(true); setError('')
    try { await officialSenders.qualify(row.school_id, row.id, { enabled: !row.enabled, revision: row.qualification_revision }); await query.refetch() }
    catch (e) { setError(e instanceof Error ? e.message : '资格更新失败') }
    finally { setBusy(false) }
  }
  return <Card title="官方发送账号">
    <Alert type="info" title="企业微信官方发送身份" description="来自已核实官方成员与平台身份映射。这里只展示成员基础资格；实际发送仍需核对目标群、官方目录和渠道权限。手机登录账号不可由平台认证，旧手机资料不会自动转换。" />
    {!global && !school && <Alert type="warning" title="请选择获授权学校" />}
    {(query.error || error) && <Alert type="error" title={error || query.error?.message} />}
    <Space><Button onClick={() => void query.refetch()} disabled={!global && !school}>刷新资格</Button><a href="/docs/guide/operator/official-senders">如何配置官方身份与学校资格</a></Space>
    <Table rowKey={row => `${row.school_id}:${row.id}`} loading={query.isPending && (global || !!school)} dataSource={query.data?.items ?? []} pagination={false} locale={{ emptyText: '没有已映射的官方发送身份。请由平台维护人员同步通讯录、核对登录身份映射和学校运营资格。' }} columns={[
      { title: '官方成员', dataIndex: 'display_name' }, { title: '官方成员标识', dataIndex: 'userid' },
      ...(global ? [{ title: '学校范围', dataIndex: 'school_id' }] : []),
      { title: '基础资格', render: (_, row) => <Tag color={row.eligible ? 'green' : 'orange'}>{row.eligible ? '基础条件满足' : '不可用'}</Tag> },
      { title: '说明', dataIndex: 'reason' },
      ...(global ? [{ title: '操作', render: (_: unknown, row: OfficialSender) => <Button disabled={busy} onClick={() => void qualify(row)}>{row.enabled ? '停用学校资格' : '启用学校资格'}</Button> }] : []),
    ]} />
    {global&&<OfficialChannelControl/>}
    <OfficialUsageManager school={school} global={global}/><BusinessRequests school={school} global={global}/>{global && <DistributionPolicyPanel />}
    <PaginationControls page={page} more={query.data?.page?.has_more} change={setPage} subject="官方发送账号" />
  </Card>
}
