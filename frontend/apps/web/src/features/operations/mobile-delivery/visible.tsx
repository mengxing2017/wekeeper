import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Space, Table, Tag } from 'antd'
import { ActionDrawer } from '../../../shared/action-drawer'
import { PaginationControls } from '../../../shared/pagination-controls'
import { Editor } from '../../admin/workspace/editor'
import {
  api,
  type SendingAccount,
  type DeliveryGroup,
  type DeliveryStrategy,
  type StrategyInput,
} from './api'
const key = () => crypto.randomUUID()
const evidence = { name: 'evidence_ref', title: '人工核对依据' }
const accountFields = [
  { name: 'company_name', title: '企业微信中可见企业名称' },
  { name: 'account_name', title: '企业微信中可见账号名称' },
  { name: 'account_detail_text', title: '账号辅助可见文字', optional: true },
]
const groupFields = [
  { name: 'name', title: '完整群名（不含成员数）' },
  {
    name: 'group_type',
    title: '群类型',
    options: [
      { value: 'external', label: '外部群' },
      { value: 'internal', label: '内部群' },
    ],
  },
  { name: 'owner_name', title: '可见群主名称', optional: true },
]
const accountProfile = (v: Record<string, string>) => ({
  company_name: v.company_name,
  account_name: v.account_name,
  account_detail_text: v.account_detail_text || null,
})
const groupProfile = (v: Record<string, string>) => ({
  group_type: v.group_type as 'external' | 'internal',
  owner_name: v.owner_name || null,
})
export function VisibleAccountCreate({ saved }: { saved: () => Promise<void> }) {
  return (
    <ActionDrawer title="登记企业微信界面账号">
      <p>按手机可见信息登记，无需企业微信 ID。保存后需人工核对。</p>
      <Editor
        fields={[
          ...accountFields,
          { name: 'responsible_actor_id', title: '本系统责任人用户 ID' },
          evidence,
        ]}
        onSave={async (v) => {
          await api.createAccount(
            {
              client_kind: 'wecom',
              identity_mode: 'ui_visible',
              visible_profile: accountProfile(v),
              responsible_actor_id: v.responsible_actor_id,
              identity_evidence_ref: v.evidence_ref,
            },
            key(),
          )
          await saved()
        }}
      />
    </ActionDrawer>
  )
}
export function VisibleGroupCreate({
  school,
  saved,
}: {
  school: string
  saved: () => Promise<void>
}) {
  return (
    <ActionDrawer title="登记企业微信界面群">
      <p>以完整群名查找，无需官方群目录或群 ID。同名无法区分时停止执行。</p>
      <Editor
        fields={[...groupFields, evidence]}
        onSave={async (v) => {
          await api.createGroup(
            school,
            {
              name: v.name,
              client_kind: 'wecom',
              identity_mode: 'ui_visible',
              visible_profile: groupProfile(v),
              identity_evidence_ref: v.evidence_ref,
            },
            key(),
          )
          await saved()
        }}
      />
    </ActionDrawer>
  )
}
export function VisibleAccountReview({
  account: a,
  saved,
}: {
  account: SendingAccount
  saved: () => Promise<void>
}) {
  if (a.identity_mode !== 'ui_visible') return null
  return (
    <Card title="可见账号资料" key={a.revision}>
      <Tag>{a.verification_state === 'reviewed' ? '已人工核对' : '待人工核对'}</Tag>
      <p>
        {a.visible_profile?.company_name} · {a.visible_profile?.account_name}（资料版本{' '}
        {a.profile_revision}）
      </p>
      <Editor
        fields={[evidence]}
        submit="已在手机核对本版本账号资料"
        onSave={async (v) => {
          await api.reviewAccountProfile(
            a.id,
            { expected_revision: a.revision, evidence_ref: v.evidence_ref },
            key(),
          )
          await saved()
        }}
      />
      <Editor
        fields={[
          ...accountFields.map((f) => ({
            ...f,
            value:
              a.visible_profile?.[
                f.name as 'company_name' | 'account_name' | 'account_detail_text'
              ] ?? '',
          })),
          { name: 'reason', title: '修改原因（修改后重新核对）' },
        ]}
        submit="更新账号可见资料"
        onSave={async (v) => {
          await api.updateAccountProfile(
            a.id,
            { expected_revision: a.revision, visible_profile: accountProfile(v), reason: v.reason },
            key(),
          )
          await saved()
        }}
      />
    </Card>
  )
}
export function VisibleGroupReview({
  school,
  group: g,
  saved,
}: {
  school: string
  group: DeliveryGroup
  saved: () => Promise<void>
}) {
  if (g.identity_mode !== 'ui_visible') return null
  return (
    <Card title="可见群资料" key={g.revision}>
      <Tag>{g.verification_state === 'reviewed' ? '已人工核对' : '待人工核对'}</Tag>
      <p>
        完整群名：{g.name} · 资料版本 {g.profile_revision}
      </p>
      <Editor
        fields={[evidence]}
        submit="已在手机核对本版本群资料"
        onSave={async (v) => {
          await api.reviewGroupProfile(
            school,
            g.id,
            { expected_revision: g.revision, evidence_ref: v.evidence_ref },
            key(),
          )
          await saved()
        }}
      />
      <Editor
        fields={[
          ...groupFields.map((f) => ({
            ...f,
            value:
              f.name === 'name'
                ? g.name
                : (g.visible_profile?.[f.name as 'group_type' | 'owner_name'] ?? ''),
          })),
          { name: 'reason', title: '修改原因（修改后重新核对）' },
        ]}
        submit="更新群可见资料"
        onSave={async (v) => {
          await api.updateGroupProfile(
            school,
            g.id,
            {
              expected_revision: g.revision,
              name: v.name,
              visible_profile: groupProfile(v),
              reason: v.reason,
            },
            key(),
          )
          await saved()
        }}
      />
    </Card>
  )
}
export function VisibleStrategies({
  school,
  openTask,
}: {
  school: string
  openTask: (id: string) => void
}) {
  const [page, setPage] = useState(1),
    [targetPage, setTargetPage] = useState(1),
    [groupPage, setGroupPage] = useState(1),
    [runPage, setRunPage] = useState(1),
    [selected, setSelected] = useState<DeliveryStrategy>(),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const lock = useRef(false),
    pending = useRef<{ strategy: string; revision: number; key: string } | undefined>(undefined)
  const cache = useQueryClient()
  const rows = useQuery({
    queryKey: ['mobile', school, 'strategies', page],
    queryFn: () => api.strategies(school, page),
  })
  const targets = useQuery({
    queryKey: ['mobile', school, 'targets', targetPage],
    queryFn: () => api.targets(school, targetPage),
  })
  const groups = useQuery({
    queryKey: ['mobile', school, 'strategy-groups', groupPage],
    queryFn: () => api.groups(school, groupPage, 100),
  })
  const runs = useQuery({
    queryKey: ['mobile', school, 'runs', selected?.id, runPage],
    queryFn: () => api.strategyRuns(school, selected!.id, runPage),
    enabled: !!selected,
  })
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', school] })
  }
  const fields = (s?: DeliveryStrategy) => [
    { name: 'name', title: '策略名称', value: s?.name },
    {
      name: 'sender_account_id',
      title: '已核对的发送账号',
      value: s?.sender_account_id,
      options:
        targets.data?.accounts
          .filter(
            (a) =>
              a.enabled && a.identity_mode === 'ui_visible' && a.verification_state === 'reviewed',
          )
          .map((a) => ({
            value: a.id,
            label: `${a.visible_profile?.company_name} · ${a.visible_profile?.account_name}`,
          })) ?? [],
    },
    {
      name: 'device_id',
      title: '绑定手机（v2，须与选择账号一致）',
      value: s?.device_id,
      options:
        targets.data?.devices
          .filter((d) => d.enabled && d.execution_protocol_version === 2)
          .map((d) => ({ value: d.id, label: `${d.name} · 绑定版本 ${d.binding_revision}` })) ?? [],
    },
    {
      name: 'group_id',
      title: '已核对的目标群',
      value: s?.group_id,
      options:
        groups.data?.items
          .filter(
            (g) =>
              g.enabled && g.identity_mode === 'ui_visible' && g.verification_state === 'reviewed',
          )
          .map((g) => ({ value: g.id, label: g.name })) ?? [],
    },
    { name: 'text_content', title: '发送正文', type: 'textarea', value: s?.text_content },
    {
      name: 'enabled',
      title: '是否允许发起新执行',
      value: String(s?.enabled ?? false),
      options: [
        { value: 'false', label: '停用' },
        { value: 'true', label: '启用（保存不发送）' },
      ],
    },
  ]
  const input = (v: Record<string, string>): StrategyInput => ({
    name: v.name,
    sender_account_id: v.sender_account_id,
    device_id: v.device_id,
    group_id: v.group_id,
    text_content: v.text_content,
    enabled: v.enabled === 'true',
    trigger_mode: 'manual_once',
  })
  async function run(s: DeliveryStrategy) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    if (
      !pending.current ||
      pending.current.strategy !== s.id ||
      pending.current.revision !== s.revision
    )
      pending.current = { strategy: s.id, revision: s.revision, key: key() }
    try {
      const result = await api.runStrategy(
        school,
        s.id,
        { expected_revision: s.revision },
        pending.current.key,
      )
      pending.current = undefined
      await refresh()
      openTask(result.task_id)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  return (
    <Card
      title="指定群发送策略"
      extra={
        <ActionDrawer title="新建发送策略">
          <Editor
            fields={fields()}
            onSave={async (v) => {
              await api.createStrategy(school, input(v), key())
              await refresh()
            }}
          />
        </ActionDrawer>
      }
    >
      <Alert
        type="info"
        title="执行一次只创建草稿，预览并人工确认后才进入手机队列。停用策略不停止已有任务。"
      />
      {[rows.error, targets.error, groups.error, runs.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {error && <Alert type="error" title={error} />}
      <Space>
        <Button onClick={() => void refresh()}>刷新策略与目标</Button>
        <Button disabled={targetPage === 1} onClick={() => setTargetPage(targetPage - 1)}>
          上一页账号与设备
        </Button>
        <Button
          disabled={!targets.data?.accounts_has_more && !targets.data?.devices_has_more}
          onClick={() => setTargetPage(targetPage + 1)}
        >
          下一页账号与设备
        </Button>
        <Button disabled={groupPage === 1} onClick={() => setGroupPage(groupPage - 1)}>
          上一页群
        </Button>
        <Button disabled={!groups.data?.page?.has_more} onClick={() => setGroupPage(groupPage + 1)}>
          下一页群
        </Button>
      </Space>
      <Table
        rowKey="id"
        pagination={false}
        dataSource={rows.data?.items ?? []}
        columns={[
          { title: '名称', dataIndex: 'name' },
          { title: '正文', dataIndex: 'text_content' },
          { title: '状态', render: (_, s) => (s.enabled ? '可执行' : '已停用') },
          {
            title: '操作',
            render: (_, s) => (
              <Space>
                <Button
                  onClick={() => {
                    setSelected(s)
                    setRunPage(1)
                  }}
                >
                  配置与历史
                </Button>
                <Button disabled={!s.enabled || busy} loading={busy} onClick={() => void run(s)}>
                  执行一次并预览
                </Button>
                <Button
                  disabled={busy}
                  onClick={() => {
                    if (lock.current) return
                    lock.current = true
                    setBusy(true)
                    setError('')
                    void api
                      .updateStrategy(
                        school,
                        s.id,
                        {
                          expected_revision: s.revision,
                          enabled: !s.enabled,
                          trigger_mode: 'manual_once',
                          name: s.name,
                          sender_account_id: s.sender_account_id,
                          group_id: s.group_id,
                          device_id: s.device_id,
                          text_content: s.text_content,
                        },
                        key(),
                      )
                      .then(async (updated) => {
                        if (selected?.id === updated.id) setSelected(updated)
                        await refresh()
                      })
                      .catch((e) => setError((e as Error).message))
                      .finally(() => {
                        lock.current = false
                        setBusy(false)
                      })
                  }}
                >
                  {s.enabled ? '停用策略' : '启用策略'}
                </Button>
              </Space>
            ),
          },
        ]}
      />
      <PaginationControls
        page={page}
        more={rows.data?.page?.has_more}
        change={setPage}
        subject="策略"
      />
      {selected && (
        <Card
          title={`策略配置 · ${selected.name}`}
          extra={<Button onClick={() => setSelected(undefined)}>关闭配置</Button>}
        >
          <Editor
            key={`${selected.id}-${selected.revision}`}
            fields={fields(selected)}
            onSave={async (v) => {
              const updated = await api.updateStrategy(
                school,
                selected.id,
                { ...input(v), expected_revision: selected.revision },
                key(),
              )
              setSelected(updated)
              await refresh()
            }}
          />
          <h3>执行历史</h3>
          {runs.data?.items.map((r) => (
            <p key={r.id}>
              策略版本 {r.strategy_revision} · {r.task_state} ·{' '}
              <Button onClick={() => openTask(r.task_id)}>查看本次任务</Button>
            </p>
          ))}
          <PaginationControls
            page={runPage}
            more={runs.data?.page?.has_more}
            change={setRunPage}
            subject="执行记录"
          />
        </Card>
      )}
    </Card>
  )
}
