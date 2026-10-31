import { createIdempotencyKey as key } from '../../../shared/idempotency-key'
import { AccountRegistrationForm, ResponsibleActorDisplay } from './account-registration'
import { DispatchWorkspace } from './dispatch'
import { ApplicationSetup } from './setup'
import { BusinessWizard } from './flows'
import { AutomationRuns } from './runs'
import { ConfigurationWorkbench } from './configuration'
import { ActionDrawer } from '../../../shared/action-drawer'
import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Drawer,
  Table,
  Input,
  Space,
  Tabs,
  Tag,
  Typography,
} from 'antd'
import { Editor } from '../../admin/workspace/editor'
import { Field } from '../../../shared/field'
import { PaginationControls } from '../../../shared/pagination-controls'
import { api, type Preview, type ItemInput } from './api'

import {
  VisibleAccountCreate,
  VisibleAccountReview,
  VisibleGroupCreate,
  VisibleGroupReview,
  VisibleStrategies,
} from './visible'

const clients = [
  { value: 'wechat', label: '微信' },
  { value: 'wecom', label: '企业微信' },
]
const identityFields = [
  { name: 'client_kind', title: '客户端', options: clients },
  { name: 'identity_namespace', title: '身份命名空间' },
  { name: 'external_key', title: '已核验的稳定身份标识' },
  { name: 'identity_evidence_ref', title: '身份核验依据' },
]
const states: Record<string, string> = {
  draft: '草稿',
  confirmed: '已确认',
  completed: '处理结束（请逐项核对结果）',
  stopped: '已停止',
  queued: '排队中',
  claimed: '已领取',
  prepared: '已准备',
  permit_issued: '已发放单次资格',
  ui_observed_sent: '界面观察到已发送（非送达证明）',
  unknown: '结果不确定，必须人工核对',
  manually_sent: '人工确认已发送',
  verified_not_sent: '人工核验未发送',
  cancelled: '已取消',
}

function useActions(scope: string) {
  const cache = useQueryClient(),
    lock = useRef(false)
  const [busy, setBusy] = useState(false),
    [error, setError] = useState('')
  async function run(action: () => Promise<unknown>) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      await action()
      await cache.invalidateQueries({ queryKey: ['mobile', scope] })
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  return { busy, run, error }
}

export function ExecutionDevices() {
  const [listTab, setListTab] = useState('devices')
  const [page, setPage] = useState(1),
    [accountPage, setAccountPage] = useState(1),
    [pairing, setPairing] = useState<{ code: string; expiresAt: string; name: string } | null>(
      null,
    ),
    [deviceId, setDeviceId] = useState(''),
    [senderId, setSenderId] = useState('')
  const devices = useQuery({
    queryKey: ['mobile', 'platform', 'devices', page],
    queryFn: () => api.devices(page),
  })
  const accounts = useQuery({
    queryKey: ['mobile', 'platform', 'accounts', accountPage],
    queryFn: () => api.accounts(accountPage),
  })
  const cache = useQueryClient(),
    actions = useActions('platform')
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', 'platform'] })
  }
  return (
    <section className="management-stack">
      {[devices.error, accounts.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {actions.error && <Alert type="error" title={actions.error} />}
      {pairing && (
        <Alert
          type="warning"
          title={`${pairing.name} · 一次性配对码`}
          description={
            <>
              <p>
                <Typography.Text strong copyable={{ text: pairing.code }}>
                  {pairing.code.slice(0, 4)} {pairing.code.slice(4)}
                </Typography.Text>
              </p>
              <p>
                在 Otter 输入这 8 位数字即可，无需输入设备 ID。有效期至{' '}
                {new Date(pairing.expiresAt).toLocaleString()}，使用一次或重新生成后失效。
              </p>
              <Button onClick={() => setPairing(null)}>关闭配对码</Button>
            </>
          }
        />
      )}

      <Tabs
        activeKey={listTab}
        onChange={setListTab}
        items={[
          { key: 'devices', label: '手机设备' },
          { key: 'accounts', label: '发送账号' },
          { key: 'rules', label: '适配管理' },
        ]}
      />
      {listTab === 'rules' ? (
        <ConfigurationWorkbench />
      ) : listTab === 'devices' ? (
        <>
          <Card
            className="data-panel"
            title="手机设备列表"
            extra={
              <Space>
                <Button onClick={() => void refresh()}>刷新设备与账号</Button>
                <ActionDrawer title="登记手机" primary>
                  <Editor
                    fields={[{ name: 'name', title: '设备名称' }]}
                    onSave={async (v) => {
                      await api.createDevice({ name: v.name }, key())
                      await refresh()
                    }}
                  />
                </ActionDrawer>
              </Space>
            }
          >
            <Table
              rowKey="id"
              loading={devices.isFetching}
              dataSource={devices.data?.items ?? []}
              pagination={false}
              scroll={{ x: 700 }}
              columns={[
                { title: '设备名称', dataIndex: 'name' },
                { title: '设备 ID', dataIndex: 'id', ellipsis: true },
                {
                  title: '状态',
                  render: (_, d) => (
                    <Tag color={d.enabled ? 'success' : 'default'}>
                      {d.enabled ? '已启用' : '已停用'}
                    </Tag>
                  ),
                },
                { title: '最近在线', render: (_, d) => d.last_seen_at ?? '尚未连接' },
                {
                  title: '操作',
                  render: (_, d) => (
                    <Button type="link" onClick={() => setDeviceId(d.id)}>
                      管理设备
                    </Button>
                  ),
                },
              ]}
            />
            <PaginationControls
              page={page}
              more={devices.data?.page?.has_more}
              change={setPage}
              subject="设备"
            />
          </Card>
          {devices.data?.items
            .filter((d) => d.id === deviceId)
            .map((d) => (
              <Drawer open size="large" key={d.id} title="设备管理" onClose={() => setDeviceId('')}>
                <Card
                  key={d.id}
                  title={d.name}
                  extra={<Tag>{d.enabled ? '已启用' : '已停用'}</Tag>}
                >
                  <p>
                    设备 ID：{d.id} · 最近在线：{d.last_seen_at ?? '尚未连接'}
                  </p>
                  <Space wrap>
                    <Button
                      disabled={actions.busy}
                      onClick={() =>
                        void actions.run(async () => {
                          const result = await api.enrollment(
                            d.id,
                            { expected_revision: d.revision },
                            key(),
                          )
                          if (!result.code) {
                            setPairing(null)
                            throw new Error('配对码无法重放，请重新生成')
                          }
                          setPairing({
                            code: result.code,
                            expiresAt: result.expires_at,
                            name: d.name,
                          })
                        })
                      }
                    >
                      生成配对码
                    </Button>
                  </Space>
                  <Editor
                    key={`state-${d.revision}`}
                    fields={[
                      {
                        name: 'enabled',
                        title: '设备状态',
                        value: String(d.enabled),
                        options: [
                          { value: 'true', label: '启用' },
                          { value: 'false', label: '停用' },
                        ],
                      },
                      { name: 'reason', title: '变更原因' },
                    ]}
                    onSave={async (v) => {
                      await api.updateDevice(
                        d.id,
                        {
                          expected_revision: d.revision,
                          enabled: v.enabled === 'true',
                          reason: v.reason,
                        },
                        key(),
                      )
                      await refresh()
                    }}
                  />
                  <Editor
                    key={`bind-${d.revision}`}
                    submit="绑定发送账号"
                    fields={[
                      {
                        name: 'sender_account_id',
                        title: '已核对发送账号',
                        options:
                          accounts.data?.items
                            .filter((a) => a.enabled)
                            .map((a) => ({
                              value: a.id,
                              label: a.visible_profile
                                ? `${a.visible_profile.company_name} · ${a.visible_profile.account_name}`
                                : a.id,
                            })) ?? [],
                      },
                      { name: 'identity_evidence_ref', title: '手机登录身份核验依据' },
                    ]}
                    onSave={async (v) => {
                      await api.bindDevice(
                        d.id,
                        {
                          expected_revision: d.revision,
                          sender_account_id: v.sender_account_id,
                          identity_evidence_ref: v.identity_evidence_ref,
                        },
                        key(),
                      )
                      await refresh()
                    }}
                  />
                  <Editor
                    key={`revoke-${d.revision}`}
                    submit="撤销设备凭据"
                    fields={[{ name: 'reason', title: '撤销原因（将使现有凭据失效）' }]}
                    onSave={async (v) => {
                      await api.revoke(
                        d.id,
                        { expected_revision: d.revision, reason: v.reason },
                        key(),
                      )
                      await refresh()
                    }}
                  />
                </Card>
              </Drawer>
            ))}
        </>
      ) : (
        <>
          <Card
            className="data-panel"
            title="真实发送账号"
            extra={
              <Space>
                <VisibleAccountCreate saved={refresh} />
                <ActionDrawer title="登记旧模式发送账号" primary>
                  <AccountRegistrationForm legacy saved={refresh} />
                </ActionDrawer>
              </Space>
            }
          >
            <Table
              rowKey="id"
              loading={accounts.isFetching}
              dataSource={accounts.data?.items ?? []}
              pagination={false}
              scroll={{ x: 700 }}
              columns={[
                {
                  title: '发送账号',
                  render: (_, a) =>
                    a.visible_profile
                      ? `${a.visible_profile.company_name} · ${a.visible_profile.account_name}`
                      : a.external_key,
                },
                { title: '责任人', render: (_, a) => <ResponsibleActorDisplay account={a} /> },
                {
                  title: '资料核对',
                  render: (_, a) =>
                    a.identity_mode === 'ui_visible' ? (
                      <Tag>{a.verification_state === 'reviewed' ? '已人工核对' : '待人工核对'}</Tag>
                    ) : (
                      '旧模式'
                    ),
                },
                {
                  title: '状态',
                  render: (_, a) => (
                    <Tag color={a.enabled ? 'success' : 'default'}>
                      {a.enabled ? '启用' : '停用'}
                    </Tag>
                  ),
                },
                { title: '每日限额', dataIndex: 'daily_limit' },
                {
                  title: '操作',
                  render: (_, a) => (
                    <Button type="link" onClick={() => setSenderId(a.id)}>
                      账号配置
                    </Button>
                  ),
                },
              ]}
            />
            <PaginationControls
              page={accountPage}
              more={accounts.data?.page?.has_more}
              change={setAccountPage}
              subject="发送账号"
            />
          </Card>
          {accounts.data?.items
            .filter((a) => a.id === senderId)
            .map((a) => (
              <Drawer
                open
                size="large"
                key={a.id}
                title="发送账号管理"
                onClose={() => setSenderId('')}
              >
                <Card key={a.id} title={a.visible_profile?.account_name ?? a.external_key}>
                  <VisibleAccountReview account={a} saved={refresh} />
                  <p>
                    责任人：
                    <ResponsibleActorDisplay account={a} /> · {a.enabled ? '启用' : '停用'}
                  </p>
                  <Editor
                    key={a.revision}
                    fields={[
                      {
                        name: 'daily_limit',
                        title:
                          a.identity_mode === 'ui_visible'
                            ? '账号独立限额（0 暂停，与发布政策取较小值）'
                            : '滚动 24 小时限额（0 暂停，最高 50）',
                        type: 'number',
                        value: a.daily_limit,
                      },
                      { name: 'reason', title: '限额变更原因' },
                    ]}
                    onSave={async (v) => {
                      await api.updateAccountPolicy(
                        a.id,
                        {
                          expected_revision: a.revision,
                          daily_limit: Number(v.daily_limit),
                          reason: v.reason,
                        },
                        key(),
                      )
                      await refresh()
                    }}
                  />
                </Card>
              </Drawer>
            ))}
        </>
      )}
    </section>
  )
}

export default function MobileDelivery({ school, role }: { school: string; role?: string }) {
  const [listTab, setListTab] = useState('tasks')
  const admin = role === 'admin',
    authorized = admin || role === 'operator'
  const [page, setPage] = useState(1),
    [groupPage, setGroupPage] = useState(1),
    [selected, setSelected] = useState(''),
    [groupId, setGroupId] = useState('')
  const capability = useQuery({
    queryKey: ['mobile', school, 'capability'],
    queryFn: () => api.capabilities(school),
    enabled: authorized,
  })
  const tasks = useQuery({
    queryKey: ['mobile', school, 'tasks', page],
    queryFn: () => api.tasks(school, page),
    enabled: authorized,
  })
  const groups = useQuery({
    queryKey: ['mobile', school, 'groups', groupPage],
    queryFn: () => api.groups(school, groupPage),
    enabled: authorized,
  })
  const cache = useQueryClient()
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', school] })
  }
  if (!authorized)
    return (
      <Alert
        type="warning"
        title="需要当前学校的运营或管理员授权；平台管理员身份不代替学校授权。"
      />
    )
  const settings = capability.data?.settings
  return (
    <section className="management-stack">
      {!capability.data?.claim_enabled && (
        <Alert
          type="warning"
          title="手机自动领取尚不可用"
          description={capability.data?.reason ?? '正在读取服务端执行开关与学校配置…'}
        />
      )}
      {[capability.error, tasks.error, groups.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}

      <Tabs
        activeKey={listTab}
        onChange={setListTab}
        items={[
          { key: 'tasks', label: '发送任务' },
          { key: 'groups', label: '目标群' },
          ...(admin
            ? [
                { key: 'applications', label: '执行设备与账号' },
                { key: 'flows', label: '业务流程' },
                { key: 'runs', label: '执行批次' },
                { key: 'legacy-applications', label: '历史 v4 应用' },
                { key: 'legacy-flows', label: '历史 v4 流程' },
                { key: 'legacy-runs', label: '历史 v4 执行' },
                { key: 'strategies', label: '历史发送策略' },
              ]
            : []),
        ]}
      />
      {admin && ['applications', 'flows', 'runs'].includes(listTab) ? (
        <DispatchWorkspace
          key={`${school}-${listTab}`}
          school={school}
          view={
            listTab === 'applications' ? 'devices' : listTab === 'flows' ? 'compose' : 'batches'
          }
          openTask={(id) => {
            setSelected(id)
            setListTab('tasks')
          }}
        />
      ) : listTab === 'legacy-applications' && admin ? (
        <ApplicationSetup school={school} />
      ) : listTab === 'legacy-flows' && admin ? (
        <BusinessWizard
          school={school}
          openTask={(id) => {
            setSelected(id)
            setListTab('tasks')
          }}
        />
      ) : listTab === 'legacy-runs' && admin ? (
        <AutomationRuns
          school={school}
          openTask={(id) => {
            setSelected(id)
            setListTab('tasks')
          }}
        />
      ) : listTab === 'strategies' && admin ? (
        <VisibleStrategies
          school={school}
          openTask={(id) => {
            setSelected(id)
            setListTab('tasks')
          }}
        />
      ) : listTab === 'groups' ? (
        <>
          <Card
            className="data-panel"
            title="目标群列表"
            extra={
              <Space>
                {admin && <VisibleGroupCreate school={school} saved={refresh} />}
                {admin && (
                  <ActionDrawer title="登记旧模式目标群" primary>
                    <Editor
                      fields={[
                        { name: 'name', title: '群名称' },
                        ...identityFields,
                        {
                          name: 'wecom_group_id',
                          title: '已有企业微信客户群 ID（普通微信群留空）',
                          optional: true,
                        },
                      ]}
                      onSave={async (v) => {
                        await api.createGroup(
                          school,
                          {
                            name: v.name,
                            client_kind: v.client_kind as 'wechat' | 'wecom',
                            identity_namespace: v.identity_namespace,
                            external_key: v.external_key,
                            identity_evidence_ref: v.identity_evidence_ref,
                            wecom_group_id: v.wecom_group_id || null,
                          },
                          key(),
                        )
                        await refresh()
                      }}
                    />
                  </ActionDrawer>
                )}
              </Space>
            }
          >
            <Table
              rowKey="id"
              loading={groups.isFetching}
              dataSource={groups.data?.items ?? []}
              pagination={false}
              scroll={{ x: 600 }}
              columns={[
                { title: '群名称', dataIndex: 'name' },
                { title: '目标群 ID', dataIndex: 'id', ellipsis: true },
                { title: '每日限额', dataIndex: 'daily_limit' },
                {
                  title: '操作',
                  render: (_, g) => (
                    <Button type="link" onClick={() => setGroupId(g.id)}>
                      群详情
                    </Button>
                  ),
                },
              ]}
            />
            <PaginationControls
              page={groupPage}
              more={groups.data?.page?.has_more}
              change={setGroupPage}
              subject="目标群"
            />
          </Card>
          {groups.data?.items
            .filter((g) => g.id === groupId)
            .map((g) => (
              <Drawer
                open
                size="large"
                key={g.id}
                title="目标群详情"
                onClose={() => setGroupId('')}
              >
                <Card key={g.id} title={g.name}>
                  {admin && <VisibleGroupReview school={school} group={g} saved={refresh} />}
                  <p>
                    目标群 ID：{g.id} · 每 24 小时限额 {g.daily_limit}
                  </p>
                  {admin && (
                    <>
                      <Editor
                        key={`bind-${g.revision}`}
                        submit="授权发送账号进入此群"
                        fields={[
                          {
                            name: 'sender_account_id',
                            title: '发送账号 ID（平台设备与发送账号页面可查）',
                          },
                          { name: 'reason', title: '授权依据' },
                        ]}
                        onSave={async (v) => {
                          await api.bindGroup(
                            school,
                            g.id,
                            {
                              sender_account_id: v.sender_account_id,
                              mapping_revision: g.revision,
                              reason: v.reason,
                            },
                            key(),
                          )
                          await refresh()
                        }}
                      />
                      <Editor
                        key={`policy-${g.revision}`}
                        fields={[
                          {
                            name: 'daily_limit',
                            title:
                              g.identity_mode === 'ui_visible'
                                ? '同群所有账号合计限额（0 暂停，与发布政策取较小值）'
                                : '同群所有账号合计限额（0 暂停，最高 30）',
                            type: 'number',
                            value: g.daily_limit,
                          },
                          { name: 'reason', title: '限额变更原因' },
                        ]}
                        onSave={async (v) => {
                          await api.updateGroupPolicy(
                            school,
                            g.id,
                            {
                              expected_revision: g.revision,
                              daily_limit: Number(v.daily_limit),
                              reason: v.reason,
                            },
                            key(),
                          )
                          await refresh()
                        }}
                      />
                    </>
                  )}
                </Card>
              </Drawer>
            ))}
        </>
      ) : (
        <>
          <Card
            className="data-panel"
            title="手机发送任务"
            extra={
              <Space>
                <Button onClick={() => void refresh()}>刷新手机任务与配置</Button>
                {admin && settings && (
                  <ActionDrawer title="学校手机执行配置">
                    <Editor
                      key={settings.revision}
                      fields={[
                        {
                          name: 'enabled',
                          title: '学校执行开关',
                          value: String(settings.enabled),
                          options: [
                            { value: 'false', label: '关闭' },
                            { value: 'true', label: '启用（仍受服务端总开关限制）' },
                          ],
                        },
                        {
                          name: 'history_reviewed',
                          title: '官方通道历史核对',
                          value: String(settings.history_reviewed),
                          options: [
                            { value: 'false', label: '尚未完成' },
                            { value: 'true', label: '已人工核对，避免重复发送' },
                          ],
                        },
                        {
                          name: 'approved_client_version',
                          title: '已批准客户端版本',
                          value: settings.approved_client_version,
                        },
                        {
                          name: 'approved_script_version',
                          title: '已批准执行程序版本',
                          value: settings.approved_script_version,
                        },
                        {
                          name: 'approved_baseline_version',
                          title: '已批准界面基线版本',
                          value: settings.approved_baseline_version,
                        },
                        {
                          name: 'evidence_ref',
                          title: '配置审核依据',
                          value: settings.evidence_ref,
                        },
                      ]}
                      onSave={async (v) => {
                        await api.updateSettings(
                          school,
                          {
                            expected_revision: settings.revision,
                            enabled: v.enabled === 'true',
                            history_reviewed: v.history_reviewed === 'true',
                            approved_client_version: v.approved_client_version,
                            approved_script_version: v.approved_script_version,
                            approved_baseline_version: v.approved_baseline_version,
                            evidence_ref: v.evidence_ref,
                          },
                          key(),
                        )
                        await refresh()
                      }}
                    />
                  </ActionDrawer>
                )}
                {capability.data?.create && (
                  <ActionDrawer title="新建手机发送草稿" primary>
                    <DraftComposer
                      school={school}
                      saved={async (id) => {
                        setSelected(id)
                        await refresh()
                      }}
                    />
                  </ActionDrawer>
                )}
              </Space>
            }
          >
            <Table
              rowKey="id"
              loading={tasks.isFetching}
              dataSource={tasks.data?.items ?? []}
              pagination={false}
              scroll={{ x: 700 }}
              columns={[
                { title: '活动标识', dataIndex: 'activity_id' },
                { title: '任务 ID', dataIndex: 'id', ellipsis: true },
                {
                  title: '状态',
                  render: (_, t) => (
                    <Tag color={t.state === 'draft' ? 'default' : 'processing'}>
                      {states[t.state]}
                    </Tag>
                  ),
                },
                {
                  title: '操作',
                  render: (_, t) => (
                    <Button type="link" onClick={() => setSelected(t.id)}>
                      查看手机任务 {t.id.slice(-6)}
                    </Button>
                  ),
                },
              ]}
            />
            <PaginationControls
              page={page}
              more={tasks.data?.page?.has_more}
              change={setPage}
              subject="手机任务"
            />
          </Card>

          {selected && (
            <Drawer open size="large" title="手机任务详情" onClose={() => setSelected('')}>
              <TaskDetail
                key={selected}
                school={school}
                id={selected}
                admin={admin}
                canConfirm={capability.data?.confirm ?? false}
              />
            </Drawer>
          )}
        </>
      )}
    </section>
  )
}

function DraftComposer({
  school,
  saved,
}: {
  school: string
  saved: (id: string) => Promise<void>
}) {
  const [items, setItems] = useState<ItemInput[]>([])
  return (
    <Card title="新建手机发送草稿">
      <p>每群分别填写内容。保存后须人工核对并确认；不确定结果不会自动重发。</p>
      <Editor
        submit="添加目标到草稿"
        fields={[
          { name: 'group_id', title: '目标群 ID（上方群列表可查）' },
          { name: 'sender_account_id', title: '已授权发送账号 ID' },
          {
            name: 'source',
            title: '内容来源',
            value: 'text',
            options: [
              { value: 'text', label: '直接文本' },
              { value: 'posts', label: '社区帖子与已发布模板' },
            ],
          },
          { name: 'text', title: '该群专属消息正文（文本模式）', type: 'textarea', optional: true },
          { name: 'template_version_id', title: '已发布模板版本 ID（帖子模式）', optional: true },
          { name: 'post_ids', title: '已审核帖子 ID（逗号分隔，帖子模式）', optional: true },
        ]}
        onSave={async (v) => {
          if (items.some((i) => i.group_id === v.group_id))
            throw new Error('同一任务不能重复添加同一群')
          if (v.source === 'text' && !v.text.trim()) throw new Error('请填写该群正文')
          const postIds = v.post_ids
            .split(',')
            .map((id) => id.trim())
            .filter(Boolean)
          if (v.source === 'posts' && (!v.template_version_id || !postIds.length))
            throw new Error('请选择模板版本和帖子')
          setItems([
            ...items,
            {
              group_id: v.group_id,
              sender_account_id: v.sender_account_id,
              source:
                v.source === 'posts'
                  ? { kind: 'posts', template_version_id: v.template_version_id, post_ids: postIds }
                  : { kind: 'text', text_content: v.text },
            },
          ])
        }}
      />
      {items.map((i, index) => (
        <p key={i.group_id}>
          {i.group_id} ·{' '}
          {i.source.kind === 'text'
            ? i.source.text_content
            : `社区帖子 ${i.source.post_ids.join(', ')}`}{' '}
          <Button onClick={() => setItems(items.filter((_, n) => n !== index))}>移除此目标</Button>
        </p>
      ))}
      <Editor
        submit="保存手机任务草稿"
        fields={[{ name: 'activity_id', title: '活动标识' }]}
        onSave={async (v) => {
          if (!items.length) throw new Error('请至少添加一个目标群')
          const task = await api.createTask(school, { activity_id: v.activity_id, items }, key())
          setItems([])
          await saved(task.id)
        }}
      />
    </Card>
  )
}

function TaskDetail({
  school,
  id,
  admin,
  canConfirm,
}: {
  school: string
  id: string
  admin: boolean
  canConfirm: boolean
}) {
  const [page, setPage] = useState(1),
    [eventPage, setEventPage] = useState(1),
    [preview, setPreview] = useState<Preview>(),
    [review, setReview] = useState(false),
    [scope, setScope] = useState(false),
    [difference, setDifference] = useState(false),
    [shared, setShared] = useState(false),
    [evidence, setEvidence] = useState('')
  const task = useQuery({
    queryKey: ['mobile', school, id],
    queryFn: () => api.task(school, id),
    refetchInterval: 15000,
  })
  const items = useQuery({
    queryKey: ['mobile', school, id, 'items', page],
    queryFn: () => api.items(school, id, page),
    refetchInterval: 15000,
  })
  const events = useQuery({
    queryKey: ['mobile', school, id, 'events', eventPage],
    queryFn: () => api.events(school, id, eventPage),
  })
  const actions = useActions(school),
    cache = useQueryClient()
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', school] })
  }
  const t = task.data
  return (
    <Card title={`手机任务详情 ${id}`}>
      <p>任务状态：{t ? states[t.state] : '正在加载'}</p>
      {[task.error, items.error, events.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {actions.error && <Alert type="error" title={actions.error} />}
      {t?.state === 'draft' && (
        <Button
          disabled={actions.busy}
          onClick={() =>
            void actions.run(async () => {
              setPreview(undefined)
              setReview(false)
              setScope(false)
              setDifference(false)
              setShared(false)
              setPreview(await api.preview(school, id, { expected_revision: t.revision }, key()))
            })
          }
        >
          生成冻结内容预览
        </Button>
      )}
      {preview && t?.state === 'draft' && (
        <section>
          <h3>逐群核对冻结内容</h3>
          {preview.items.map((i) => (
            <Card key={i.id} title={i.group_name ?? i.group_id}>
              <p>
                {i.sequence ? `第 ${i.sequence.position}/${i.sequence.total} 群` : null} · 配置{' '}
                {i.configuration?.id ?? '旧协议'}
              </p>
              {i.configuration?.configuration && (
                <p>
                  群间等待{' '}
                  {(i.sequence?.group_interval_ms ??
                    i.configuration.configuration.policy.default_group_interval_ms) / 1000}{' '}
                  秒 · 人工最低间隔{' '}
                  {i.configuration.configuration.baseline.human_group_min_interval_ms / 1000} 秒 ·
                  基准版本 {i.configuration.configuration.baseline.version}
                </p>
              )}
              <pre className="text-content">{i.content}</pre>
              {i.reason && (
                <Alert
                  type="warning"
                  title={i.reason}
                  description={
                    i.next_eligible_at ? `最早可执行时间：${i.next_eligible_at}` : undefined
                  }
                />
              )}
            </Card>
          ))}
          <Checkbox checked={review} onChange={(e) => setReview(e.target.checked)}>
            已人工审核各群内容
          </Checkbox>
          <Checkbox checked={scope} onChange={(e) => setScope(e.target.checked)}>
            已核对目标群、授权和发送范围
          </Checkbox>
          {preview.requires_substantive_difference_review && (
            <Checkbox checked={difference} onChange={(e) => setDifference(e.target.checked)}>
              已核对同校不同群内容的实质差异
            </Checkbox>
          )}
          {preview.requires_shared_text_scope_acceptance && (
            <Checkbox checked={shared} onChange={(e) => setShared(e.target.checked)}>
              已确认本次有序目标可接收相同正文，并核对群顺序、配置及人工基准
            </Checkbox>
          )}
          <Field label="人工确认依据">
            <Input value={evidence} onChange={(e) => setEvidence(e.target.value)} />
          </Field>
          <Button
            type="primary"
            loading={actions.busy}
            disabled={
              !canConfirm ||
              (!admin && items.data?.items.some((i) => i.source.kind === 'text')) ||
              !review ||
              !scope ||
              !evidence.trim() ||
              (preview.requires_substantive_difference_review && !difference) ||
              (preview.requires_shared_text_scope_acceptance && !shared) ||
              preview.expected_revision !== t.revision
            }
            onClick={() =>
              void actions.run(async () => {
                await api.confirm(
                  school,
                  id,
                  {
                    expected_revision: preview.expected_revision,
                    preview_digest: preview.preview_digest,
                    content_review: review,
                    scope_acceptance: scope,
                    substantive_difference_reviewed: difference,
                    ...(preview.requires_shared_text_scope_acceptance
                      ? { shared_text_scope_acceptance: shared }
                      : {}),
                    evidence_ref: evidence,
                  },
                  key(),
                )
                setPreview(undefined)
              })
            }
          >
            人工确认并进入执行队列
          </Button>
          {!admin && <p>直接文本草稿须由当前学校管理员确认。</p>}
        </section>
      )}
      {admin && t && !['stopped', 'completed'].includes(t.state) && (
        <Editor
          key={t.revision}
          submit="停止后续执行"
          fields={[{ name: 'reason', title: '停止原因' }]}
          onSave={async (v) => {
            await api.stop(school, id, { expected_revision: t.revision, reason: v.reason }, key())
            setPreview(undefined)
            await refresh()
          }}
        />
      )}
      <h3>逐群执行记录</h3>
      {items.data?.items.map((i) => (
        <Card
          key={i.id}
          title={i.snapshot.group?.name ?? i.group_id}
          extra={<Tag>{states[i.state]}</Tag>}
        >
          <pre className="text-content">{i.content}</pre>
          <p>
            账号：
            {i.snapshot.sender?.visible_profile
              ? `${i.snapshot.sender.visible_profile.company_name} · ${i.snapshot.sender.visible_profile.account_name}`
              : i.sender_account_id}
          </p>
          {i.snapshot.group?.name && <p>完整群名：{i.snapshot.group.name}</p>}
          {i.snapshot.execution_target && (
            <p>
              指定手机：{i.snapshot.execution_target.device_id} · 绑定版本：
              {i.snapshot.execution_target.device_binding_revision} · 策略版本：
              {i.snapshot.execution_target.strategy_revision}
            </p>
          )}
          {admin && i.state === 'unknown' && (
            <Editor
              key={i.revision}
              submit="提交人工核对（不会自动重发）"
              fields={[
                {
                  name: 'decision',
                  title: '核对结论',
                  options: [
                    { value: 'manually_sent', label: '人工确认已发送' },
                    { value: 'verified_not_sent', label: '已核验未发送' },
                  ],
                },
                { name: 'reason', title: '核对说明' },
                { name: 'evidence_ref', title: '核对证据' },
              ]}
              onSave={async (v) => {
                await api.reconcile(
                  school,
                  i.id,
                  {
                    expected_revision: i.revision,
                    decision: v.decision as 'manually_sent' | 'verified_not_sent',
                    reason: v.reason,
                    evidence_ref: v.evidence_ref,
                  },
                  key(),
                )
                await refresh()
              }}
            />
          )}
        </Card>
      ))}
      <PaginationControls
        page={page}
        more={items.data?.page?.has_more}
        change={setPage}
        subject="执行记录"
      />
      <h3>操作事件</h3>
      {events.data?.items.map((e) => (
        <p key={e.id}>
          {e.created_at} · {e.action}
        </p>
      ))}
      <PaginationControls
        page={eventPage}
        more={events.data?.page?.has_more}
        change={setEventPage}
        subject="操作事件"
      />
    </Card>
  )
}
