import { createIdempotencyKey as key } from '../../../shared/idempotency-key'
import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Space, Table, Tag, Select, InputNumber, Form, Input } from 'antd'
import { ActionDrawer } from '../../../shared/action-drawer'
import { PaginationControls } from '../../../shared/pagination-controls'
import { Editor } from '../../admin/workspace/editor'
import { AccountRegistrationForm, EvidenceField, ProfileForm } from './account-registration'
import { formatDate } from '../../../shared/format-date'
import {
  api,
  type SendingAccount,
  type DeliveryGroup,
  type DeliveryStrategy,
  type StrategyInput,
  type ConfigurationRelease,
  type RunRevisionPreview,
} from './api'
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
      <p>按手机可见信息登记，无需企业微信 ID。保存后为待人工核对，登记依据不会自动完成核对。</p>
      <AccountRegistrationForm saved={saved} />
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
      <p>登记依据只记录来源；保存后仍需人工核对本版本群资料。</p>
      <ProfileForm onSave={async (v, requestKey) => {
        await api.createGroup(school, {
          name: v.name, client_kind: 'wecom', identity_mode: 'ui_visible',
          visible_profile: groupProfile(v), identity_evidence_ref: v.evidence_ref,
        }, requestKey)
        await saved()
      }}>
        <Form.Item name="name" label="完整群名（不含成员数）" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item>
        <Form.Item name="group_type" label="群类型" rules={[{ required: true }]}><Select options={groupFields[1].options} /></Form.Item>
        <Form.Item name="owner_name" label="可见群主名称"><Input /></Form.Item>
        <EvidenceField registration />
      </ProfileForm>
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
      <p>登记依据：{a.registration_evidence_ref ?? '历史记录未独立保存登记依据'}</p>
      {a.latest_profile_review && <p>
        最近一次核对：{a.latest_profile_review.actor_name}（{a.latest_profile_review.actor_username}）
        {' · '}{formatDate(a.latest_profile_review.reviewed_at)}
        {' · '}资料版本 {a.latest_profile_review.profile_revision ?? '历史版本未记录'}
        {' · '}依据：{a.latest_profile_review.evidence_ref}
        {a.verification_state !== 'reviewed' && '（历史核对，不适用于当前待核对资料）'}
      </p>}
      {!a.latest_profile_review && a.verification_state === 'reviewed' && <p>历史核对依据：{a.identity_evidence_ref}（核对人及时间未完整记录）</p>}
      <ProfileForm
        key={`review-${a.revision}`}
        submit="已在手机核对本版本账号资料"
        onSave={async (v, requestKey) => {
          await api.reviewAccountProfile(a.id, { expected_revision: a.revision, evidence_ref: v.evidence_ref }, requestKey)
          await saved()
        }}
      ><EvidenceField /></ProfileForm>
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
      <p>已保存依据（仅供参考）：{g.identity_evidence_ref}</p>
      <ProfileForm
        key={`review-${g.revision}`}
        submit="已在手机核对本版本群资料"
        onSave={async (v, requestKey) => {
          await api.reviewGroupProfile(school, g.id, { expected_revision: g.revision, evidence_ref: v.evidence_ref }, requestKey)
          await saved()
        }}
      ><EvidenceField /></ProfileForm>
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
function StrategyEditor({
  fields,
  strategy,
  groups,
  releases,
  onSave,
}: {
  fields: Parameters<typeof Editor>[0]['fields']
  strategy?: DeliveryStrategy
  groups: DeliveryGroup[]
  releases: ConfigurationRelease[]
  onSave: (v: Record<string, string>, extra: Partial<StrategyInput>) => Promise<void>
}) {
  const [mode, setMode] = useState(strategy?.delivery_mode ?? 'single_group')
  const [targets, setTargets] = useState<string[]>(strategy?.targets?.map((t) => t.group_id) ?? [])
  const [release, setRelease] = useState(strategy?.configuration_release_id ?? undefined)
  const [interval, setInterval] = useState<number | null>(
    strategy?.inter_group_interval_seconds ?? null,
  )
  return (
    <Space orientation="vertical" className="full-width">
      <label>
        发送模式{' '}
        <Select
          value={mode}
          onChange={setMode}
          options={[
            { value: 'single_group', label: '单群' },
            { value: 'ordered_groups', label: '多群顺序发送' },
          ]}
        />
      </label>
      <label>
        已发布的规则与政策{' '}
        <Select
          allowClear
          style={{ minWidth: 300 }}
          value={release}
          onChange={setRelease}
          options={releases
            .filter(
              (r) =>
                r.active &&
                !r.revoked &&
                (!strategy || !r.device_id || r.device_id === strategy.device_id),
            )
            .map((r) => ({
              value: r.id,
              label: `${r.name} · ${r.client?.version_name} · ${r.id}`,
            }))}
        />
      </label>
      <label>
        群间等待（秒，留空使用发布政策默认值）{' '}
        <InputNumber min={0} value={interval} onChange={setInterval} />
      </label>
      {mode === 'ordered_groups' && (
        <>
          <Select<string>
            aria-label="添加目标群"
            placeholder="依次添加已核对的群"
            value={undefined}
            style={{ minWidth: 300 }}
            options={groups
              .filter((g) => !targets.includes(g.id))
              .map((g) => ({ value: g.id, label: g.name }))}
            onChange={(id) => setTargets([...targets, id])}
          />
          {targets.map((id, index) => (
            <Space key={id}>
              <span>
                {index + 1}. {groups.find((g) => g.id === id)?.name ?? id}
              </span>
              <Button
                disabled={!index}
                onClick={() => {
                  const copy = [...targets]
                  ;[copy[index - 1], copy[index]] = [copy[index]!, copy[index - 1]!]
                  setTargets(copy)
                }}
              >
                上移
              </Button>
              <Button
                disabled={index === targets.length - 1}
                onClick={() => {
                  const copy = [...targets]
                  ;[copy[index + 1], copy[index]] = [copy[index]!, copy[index + 1]!]
                  setTargets(copy)
                }}
              >
                下移
              </Button>
              <Button onClick={() => setTargets(targets.filter((t) => t !== id))}>移除</Button>
            </Space>
          ))}
        </>
      )}
      <Editor
        fields={fields.filter((f) => mode === 'single_group' || f.name !== 'group_id')}
        onSave={async (v) => {
          if (mode === 'ordered_groups' && (!release || targets.length < 2))
            throw new Error('多群模式需选择发布配置及至少两个不同群')
          await onSave(v, {
            delivery_mode: mode,
            group_id: mode === 'single_group' ? v.group_id : null,
            targets: mode === 'ordered_groups' ? targets.map((group_id) => ({ group_id })) : [],
            configuration_release_id: release ?? null,
            inter_group_interval_seconds: interval,
          })
        }}
      />
    </Space>
  )
}
function RunDetails({
  school,
  id,
  releases,
}: {
  school: string
  id: string
  releases: ConfigurationRelease[]
}) {
  const [target, setTarget] = useState<string>(),
    [preview, setPreview] = useState<RunRevisionPreview>(),
    [evidence, setEvidence] = useState(''),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const progress = useQuery({
    queryKey: ['mobile', school, 'progress', id],
    queryFn: () => api.runProgress(school, id),
    refetchInterval: 15000,
  })
  const action = async (fn: () => Promise<unknown>) => {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      await fn()
      await progress.refetch()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const p = progress.data
  return (
    <Card title="顺序执行进度">
      {(error || progress.error) && <Alert type="error" title={error || progress.error?.message} />}
      {p && (
        <>
          <p>
            当前第 {p.current_position} 项 / {p.items.length} 项 · 执行修订 {p.execution_revision} ·{' '}
            {p.items.every((i) => ['ui_observed_sent', 'manually_sent'].includes(i.state))
              ? '全部群已确认发送'
              : '尚未全部成功'}
          </p>
          <p>
            等待至：{p.next_eligible_at ?? '无'} · 暂停原因：{p.pause_reason ?? '无'} · 群间隔：
            {p.inter_group_interval_ms / 1000} 秒
          </p>
          <Table
            rowKey="id"
            pagination={false}
            dataSource={p.items}
            columns={[
              { title: '顺序', dataIndex: 'position' },
              { title: '群', render: (_, i) => i.snapshot.group?.name ?? i.group_id },
              { title: '状态', dataIndex: 'state' },
            ]}
          />
          <p>
            修订只作用于尚未领取、未发许可的剩余项；已发送项和历史快照保留。请先暂停手机，再预览并确认修订。
          </p>
          <Select
            aria-label="替换配置"
            placeholder="选择已发布的规则版本"
            value={target}
            style={{ minWidth: 300 }}
            onChange={(v) => {
              setTarget(v)
              setPreview(undefined)
            }}
            options={releases
              .filter((r) => r.active && !r.revoked)
              .map((r) => ({ value: r.id, label: `${r.name} · ${r.id}` }))}
          />
          <Button
            disabled={!target || busy}
            onClick={() =>
              void action(async () =>
                setPreview(await api.previewRunRevision(school, id, target!, key())),
              )
            }
          >
            预览剩余项修订
          </Button>
          {preview && (
            <>
              <pre>
                当前进度版本：{preview.revision}
                新配置：{preview.new_release.id}
                预览摘要：{preview.preview_digest}
              </pre>
              <Editor
                fields={[{ name: 'evidence_ref', title: '人工核对与恢复依据', value: evidence }]}
                submit="确认修订并允许后续领取"
                onSave={async (v) => {
                  setEvidence(v.evidence_ref)
                  await api.reviseRun(
                    school,
                    id,
                    {
                      expected_revision: preview.revision,
                      configuration_release_id: target!,
                      preview_digest: preview.preview_digest,
                      evidence_ref: v.evidence_ref,
                    },
                    key(),
                  )
                  setPreview(undefined)
                  await progress.refetch()
                }}
              />
            </>
          )}
        </>
      )}
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
    [runId, setRunId] = useState<string>(),
    [releasePage, setReleasePage] = useState(1),
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
  const releases = useQuery({
    queryKey: ['mobile', school, 'releases', releasePage],
    queryFn: () => api.schoolConfigurationReleases(school, releasePage, 100),
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
      title: '绑定手机（须与选择账号、发布范围一致）',
      value: s?.device_id,
      options:
        targets.data?.devices
          .filter((d) => d.enabled && (d.execution_protocol_version ?? 1) >= 2)
          .map((d) => ({ value: d.id, label: `${d.name} · 绑定版本 ${d.binding_revision}` })) ?? [],
    },
    {
      name: 'group_id',
      title: '已核对的目标群',
      value: s?.group_id ?? undefined,
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
          <StrategyEditor
            fields={fields()}
            groups={
              groups.data?.items.filter(
                (g) =>
                  g.enabled &&
                  g.identity_mode === 'ui_visible' &&
                  g.verification_state === 'reviewed',
              ) ?? []
            }
            releases={releases.data?.items ?? []}
            onSave={async (v, extra) => {
              await api.createStrategy(school, { ...input(v), ...extra }, key())
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
      <PaginationControls
        page={releasePage}
        more={releases.data?.page?.has_more}
        change={setReleasePage}
        subject="发布配置"
      />
      {runId && (
        <RunDetails key={runId} school={school} id={runId} releases={releases.data?.items ?? []} />
      )}
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
                          delivery_mode: s.delivery_mode,
                          targets: s.targets,
                          configuration_release_id: s.configuration_release_id,
                          inter_group_interval_seconds: s.inter_group_interval_seconds,
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
          <StrategyEditor
            key={`${selected.id}-${selected.revision}`}
            strategy={selected}
            groups={
              groups.data?.items.filter(
                (g) =>
                  g.enabled &&
                  g.identity_mode === 'ui_visible' &&
                  g.verification_state === 'reviewed',
              ) ?? []
            }
            releases={releases.data?.items ?? []}
            fields={fields(selected)}
            onSave={async (v, extra) => {
              const updated = await api.updateStrategy(
                school,
                selected.id,
                { ...input(v), ...extra, expected_revision: selected.revision },
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
              <Button onClick={() => setRunId(r.id)}>逐群进度与配置修订</Button>
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
