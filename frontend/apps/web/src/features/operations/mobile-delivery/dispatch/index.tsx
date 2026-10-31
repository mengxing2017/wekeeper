import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Drawer,
  Input,
  InputNumber,
  Select,
  Space,
  Steps,
  Table,
  Tag,
} from 'antd'
import { api, type DispatchBatch, type DispatchInput } from '../api'
import { createIdempotencyKey as key } from '../../../../shared/idempotency-key'
import { PaginationControls } from '../../../../shared/pagination-controls'

const states: Record<string, string> = {
  draft: '待核对',
  confirmed: '已确认',
  paused: '已暂停',
  cancelled: '已停止',
  queued: '等待执行',
  claimed: '执行中',
  prepared: '发送准备完成',
  permit_issued: '单次发送资格已发放',
  ui_observed_sent: '界面观察已发送',
  unknown: '发送不明，须人工核对',
  manually_sent: '人工确认已发',
  verified_not_sent: '人工核验未发',
}
export function DispatchWorkspace({
  school,
  view,
  openTask,
}: {
  school: string
  view: 'devices' | 'compose' | 'batches'
  openTask: (id: string) => void
}) {
  const cache = useQueryClient(),
    lock = useRef(false),
    confirmation = useRef('')
  const [historyReviewed, setHistoryReviewed] = useState(false)
  const [error, setError] = useState(''),
    [busy, setBusy] = useState(false),
    [page, setPage] = useState(1),
    [groupPage, setGroupPage] = useState(1),
    [detailId, setDetailId] = useState('')
  const [devices, setDevices] = useState<string[]>([]),
    [groupId, setGroupId] = useState<string>(),
    [releaseId, setReleaseId] = useState<string>(),
    [groupName, setGroupName] = useState('')
  const [input, setInput] = useState<DispatchInput>({
      name: '企业微信群消息',
      text_content: '',
      interval_seconds: 1,
      targets: [],
    }),
    [step, setStep] = useState(0),
    [preview, setPreview] = useState<DispatchBatch>(),
    [reviewed, setReviewed] = useState(false)
  const targets = useQuery({
    queryKey: ['mobile', school, 'dispatch-targets', page],
    queryFn: () => api.targets(school, page),
  })
  const assignments = useQuery({
    queryKey: ['mobile', school, 'dispatch-assignments', page],
    queryFn: () => api.dispatch.assignments(school, page),
  })
  const releases = useQuery({
    queryKey: ['mobile', school, 'dispatch-releases', page],
    queryFn: () => api.dispatch.releases(school, page),
  })
  const deviceGroups = useQuery({
    queryKey: ['mobile', school, 'dispatch-groups', page],
    queryFn: () => api.dispatch.groups(school, page),
  })
  const groups = useQuery({
    queryKey: ['mobile', school, 'dispatch-message-groups', groupPage],
    queryFn: () => api.groups(school, groupPage, 100),
  })
  const batches = useQuery({
    queryKey: ['mobile', school, 'dispatch-batches', page],
    queryFn: () => api.dispatch.batches(school, page),
    enabled: view === 'batches',
    refetchInterval: 10000,
  })
  const detail = useQuery({
    queryKey: ['mobile', school, 'dispatch-batch', detailId],
    queryFn: () => api.dispatch.batch(school, detailId),
    enabled: !!detailId,
    refetchInterval: 5000,
  })
  const failures = useQuery({
    queryKey: ['mobile', school, 'dispatch-failures', page],
    queryFn: () => api.dispatch.failures(school, page),
    enabled: view === 'batches',
  })
  const allAssignments = useRef(
    new Map<string, NonNullable<typeof assignments.data>['items'][number]>(),
  )
  assignments.data?.items.forEach((a) => allAssignments.current.set(a.id, a))
  const groupNames = useRef(new Map<string, string>())
  groups.data?.items.forEach((g) => groupNames.current.set(g.id, g.name))
  const selected = input.targets
    .map((t) => allAssignments.current.get(t.assignment_id))
    .filter((a) => a != null)
  const minimum = Math.ceil(
    Math.max(
      0,
      ...selected.map((a) =>
        Math.max(
          a.flow.configuration.policy.minimum_group_interval_ms,
          a.flow.configuration.baseline.human_group_min_interval_ms,
        ),
      ),
    ) / 1000,
  )
  async function run(action: () => Promise<void>) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      await action()
      await cache.invalidateQueries({ queryKey: ['mobile', school] })
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  function change(value: DispatchInput) {
    setInput(value)
    setPreview(undefined)
    setReviewed(false)
    confirmation.current = ''
  }
  function showPreview(value: DispatchBatch) {
    setPreview(value)
    setReviewed(false)
    confirmation.current = key()
    setStep(2)
  }
  const queries = [targets, assignments, releases, deviceGroups, groups, batches, detail, failures]
  const review = preview && (
    <Card title="核对设备、账号、正文与顺序">
      <p>
        {preview.name} · 每群发送结果确认后至少等待 {preview.input.interval_seconds}{' '}
        秒，再搜索下一群
      </p>
      <p style={{ whiteSpace: 'pre-wrap' }}>{preview.input.text_content}</p>
      {preview.preview.targets.map((t) => (
        <div key={t.assignment.id}>
          <strong>
            {t.assignment.device_name} / {t.assignment.account_name}
          </strong>
          <ol>
            {t.groups.map((g) => (
              <li key={g.group.id}>{g.group.name}</li>
            ))}
          </ol>
        </div>
      ))}
      <Checkbox checked={reviewed} onChange={(e) => setReviewed(e.target.checked)}>
        我已核对各群唯一执行归属、内容和顺序，并确认本次业务发送
      </Checkbox>
      <Button
        type="primary"
        loading={busy}
        disabled={!reviewed}
        onClick={() =>
          void run(async () => {
            const b = await api.dispatch.confirm(school, preview, confirmation.current)
            setPreview(undefined)
            setDetailId(b.id)
            setStep(0)
            change({ ...input, targets: [] })
          })
        }
      >
        确认并排队
      </Button>
    </Card>
  )
  return (
    <Space orientation="vertical" className="full-width">
      <Alert
        type="info"
        title="企业微信群纯文本 · 通用流程 v5"
        description="手机已配对并分配业务账号即可排队；离线、未知应用版本或未上报安装环境均不阻止创建。每个群只分给一个执行者，逐群发送并持久等待。"
      />
      {queries.map((q, i) => q.error && <Alert key={i} type="error" title={q.error.message} />)}
      {error && <Alert type="error" title={error} />}
      {view === 'devices' && (
        <>
          <Card title="本校执行配置">
            <Space wrap>
              <a href={`/admin?school=${encodeURIComponent(school)}&view=execution-devices&scope=school`}>查看设备</a>
              <a href={`/admin?school=${encodeURIComponent(school)}&view=execution-accounts&scope=school`}>查看发送账号</a>
            </Space>
            <p>此处配置本校使用关系，不重复登记设备。没有可选资源时，请联系维护人员完成配对、账号核验及学校授权。</p>
            <Table
              rowKey="id"
              pagination={false}
              dataSource={targets.data?.devices ?? []}
              rowSelection={{
                selectedRowKeys: devices,
                onChange: (ids) => setDevices(ids.map(String)),
                preserveSelectedRowKeys: true,
              }}
              columns={[
                { title: '手机', dataIndex: 'name' },
                {
                  title: '账号',
                  render: (_, d) =>
                    targets.data?.accounts.find((a) => a.id === d.sender_account_id)
                      ?.visible_profile?.account_name ?? '已分配账号',
                },
                {
                  title: '接入状态',
                  render: (_, d) => (
                    <Tag>{d.execution_protocol_version === 5 ? 'v5 可接收' : '待升级后执行'}</Tag>
                  ),
                },
              ]}
            />
            <PaginationControls
              page={page}
              change={setPage}
              more={targets.data?.devices_has_more ?? false}
            />
            <Input
              aria-label="设备组名称"
              placeholder="设备组名称"
              value={groupName}
              onChange={(e) => setGroupName(e.target.value)}
            />
            <Button
              disabled={!devices.length || !groupName}
              loading={busy}
              onClick={() =>
                void run(async () => {
                  await api.dispatch.saveGroup(school, groupName, devices, key())
                  setGroupName('')
                })
              }
            >
              将所选手机保存为设备组
            </Button>
          </Card>
          <Card title="批量分配流程">
            <Checkbox
              checked={historyReviewed}
              onChange={(e) => setHistoryReviewed(e.target.checked)}
            >
              已核对本校历史与其他通道的未决发送，再开启手机执行
            </Checkbox>
            <Space wrap>
              <Select
                allowClear
                aria-label="执行设备组"
                placeholder="选择设备组，也可使用上方手机"
                value={groupId}
                onChange={setGroupId}
                options={deviceGroups.data?.items.map((g) => ({
                  value: g.id,
                  label: `${g.name}（${g.device_ids.length} 台）`,
                }))}
              />
              <Select
                aria-label="已发布通用流程"
                placeholder="选择已发布流程"
                value={releaseId}
                onChange={setReleaseId}
                options={releases.data?.items
                  .filter((r) => r.active && !r.revoked)
                  .map((r) => ({ value: r.id, label: r.name }))}
              />
              <Button
                disabled={!releaseId || (!devices.length && !groupId)}
                loading={busy}
                onClick={() =>
                  void run(async () => {
                    await api.dispatch.assign(school, devices, groupId ?? null, releaseId!, key())
                  })
                }
              >
                为所选范围分配
              </Button>
              <Button
                loading={busy}
                disabled={!historyReviewed}
                onClick={() =>
                  void run(async () => {
                    await api.dispatch.enabled(school, true, key())
                  })
                }
              >
                启用学校 v5 执行
              </Button>
              <Button
                loading={busy}
                onClick={() =>
                  void run(async () => {
                    await api.dispatch.enabled(school, false, key())
                  })
                }
              >
                关闭学校 v5 执行
              </Button>
            </Space>
            <p>账号及学校范围沿用后台登记。设备组展开后冻结，后续增减成员不会改动已确认批次。</p>
          </Card>
          <Table
            rowKey="id"
            pagination={false}
            dataSource={assignments.data?.items ?? []}
            columns={[
              { title: '手机', dataIndex: 'device_name' },
              { title: '业务账号', dataIndex: 'account_name' },
              { title: '流程', dataIndex: 'flow_name' },
              {
                title: '连接',
                render: (_, a) => <Tag>{a.online ? '在线' : '离线，任务可排队'}</Tag>,
              },
              { title: '执行端', render: (_, a) => (a.protocol === 5 ? 'v5' : '待升级') },
            ]}
          />
        </>
      )}
      {view === 'compose' && (
        <>
          <Steps
            current={step}
            items={[{ title: '选择范围' }, { title: '填写消息' }, { title: '核对并确认' }]}
          />
          {step === 0 && (
            <Card title="选择执行手机与账号">
              <Select
                mode="multiple"
                aria-label="执行范围"
                placeholder="选择已分配流程的设备与账号"
                value={input.targets.map((t) => t.assignment_id)}
                options={assignments.data?.items
                  .filter((a) => a.enabled)
                  .map((a) => ({
                    value: a.id,
                    label: `${a.device_name} / ${a.account_name} · ${a.flow_name}${a.online ? '' : ' · 离线可排队'}${a.protocol === 5 ? '' : ' · 待升级'}`,
                  }))}
                onChange={(ids) =>
                  change({
                    ...input,
                    targets: ids.map(
                      (assignment_id) =>
                        input.targets.find((t) => t.assignment_id === assignment_id) ?? {
                          assignment_id,
                          group_ids: [],
                        },
                    ),
                  })
                }
              />
              <PaginationControls
                page={page}
                change={setPage}
                more={assignments.data?.page?.has_more ?? false}
              />
              <Button disabled={!input.targets.length} onClick={() => setStep(1)}>
                填写消息
              </Button>
            </Card>
          )}
          {step === 1 && (
            <>
              <Card title="消息与节奏">
                <Input
                  aria-label="批次名称"
                  value={input.name}
                  onChange={(e) => change({ ...input, name: e.target.value })}
                />
                <Input.TextArea
                  aria-label="发送正文"
                  value={input.text_content}
                  onChange={(e) => change({ ...input, text_content: e.target.value })}
                />
                <p>有效人工下限：{minimum} 秒；可延长。</p>
                <InputNumber
                  aria-label="群间等待秒数"
                  min={minimum}
                  value={input.interval_seconds}
                  onChange={(n) => change({ ...input, interval_seconds: n ?? minimum })}
                />{' '}
                秒
              </Card>
              {input.targets.map((t) => (
                <Card
                  key={t.assignment_id}
                  title={`${allAssignments.current.get(t.assignment_id)?.device_name} / ${allAssignments.current.get(t.assignment_id)?.account_name}`}
                >
                  <Select
                    mode="multiple"
                    aria-label={`有序群-${t.assignment_id}`}
                    value={t.group_ids}
                    options={groups.data?.items
                      .filter((g) => g.identity_mode === 'ui_visible' && g.enabled)
                      .map((g) => ({
                        value: g.id,
                        label: g.name,
                        disabled: input.targets.some(
                          (x) => x.assignment_id !== t.assignment_id && x.group_ids.includes(g.id),
                        ),
                      }))}
                    onChange={(group_ids) =>
                      change({
                        ...input,
                        targets: input.targets.map((x) => (x === t ? { ...x, group_ids } : x)),
                      })
                    }
                  />
                  <ol>
                    {t.group_ids.map((g, i) => (
                      <li key={g}>
                        {groupNames.current.get(g) ?? '已选择的群'}{' '}
                        <Button
                          size="small"
                          disabled={i === 0}
                          onClick={() => {
                            const v = [...t.group_ids]
                            ;[v[i - 1], v[i]] = [v[i], v[i - 1]]
                            change({
                              ...input,
                              targets: input.targets.map((x) =>
                                x === t ? { ...x, group_ids: v } : x,
                              ),
                            })
                          }}
                        >
                          上移
                        </Button>
                        <Button
                          size="small"
                          onClick={() =>
                            change({
                              ...input,
                              targets: input.targets.map((x) =>
                                x === t
                                  ? { ...x, group_ids: x.group_ids.filter((id) => id !== g) }
                                  : x,
                              ),
                            })
                          }
                        >
                          移除
                        </Button>
                      </li>
                    ))}
                  </ol>
                </Card>
              ))}
              <PaginationControls
                page={groupPage}
                change={setGroupPage}
                more={groups.data?.page?.has_more ?? false}
              />
              <Space>
                <Button onClick={() => setStep(0)}>返回范围</Button>
                <Button
                  loading={busy}
                  disabled={
                    !input.text_content ||
                    input.interval_seconds < minimum ||
                    input.targets.some((t) => !t.group_ids.length)
                  }
                  onClick={() =>
                    void run(async () =>
                      showPreview(await api.dispatch.preview(school, input, key())),
                    )
                  }
                >
                  生成核对预览
                </Button>
              </Space>
            </>
          )}
          {step === 2 && review}
          <Button
            onClick={() => {
              setStep(0)
              setPreview(undefined)
              setReviewed(false)
            }}
          >
            重新填写
          </Button>
        </>
      )}
      {view === 'batches' && (
        <>
          {review}
          <Table
            rowKey="id"
            pagination={false}
            dataSource={batches.data?.items ?? []}
            columns={[
              { title: '批次', dataIndex: 'name' },
              { title: '状态', render: (_, b) => states[b.state] },
              {
                title: '操作',
                render: (_, b) => <Button onClick={() => setDetailId(b.id)}>查看子任务</Button>,
              },
            ]}
          />
          <PaginationControls
            page={page}
            change={setPage}
            more={batches.data?.page?.has_more ?? false}
          />
          <Card title="执行失败与适配">
            <Table
              rowKey="id"
              pagination={false}
              dataSource={failures.data?.items ?? []}
              columns={[
                { title: '步骤', render: (_, f) => `${f.body.path_key} / ${f.body.step}` },
                { title: '原因', render: (_, f) => f.body.reason },
                { title: '同类次数', dataIndex: 'occurrences' },
                {
                  title: '发送状态',
                  render: (_, f) => (f.body.send_state === 'unknown' ? '不明，须核对' : '明确未发'),
                },
                {
                  title: '失败时版本',
                  render: (_, f) =>
                    [
                      f.body.versions?.application_name,
                      f.body.versions?.android,
                      f.body.versions?.launcher,
                    ]
                      .filter(Boolean)
                      .join(' / ') || '无法读取版本，已保留失败',
                },
              ]}
            />
          </Card>
        </>
      )}
      <Drawer
        open={!!detailId}
        size={820}
        title={detail.data?.name ?? '执行批次'}
        onClose={() => setDetailId('')}
      >
        {detail.data && (
          <>
            <p>{states[detail.data.state]}</p>
            <Space>
              {(['pause', 'resume', 'stop'] as const).map((command) => (
                <Button
                  key={command}
                  disabled={detail.data?.state === 'cancelled' || detail.data?.state === 'draft'}
                  loading={busy}
                  onClick={() =>
                    void run(async () => {
                      await api.dispatch.control(school, detail.data!, command, key())
                    })
                  }
                >
                  {{ pause: '暂停批次', resume: '恢复批次', stop: '停止批次' }[command]}
                </Button>
              ))}
            </Space>
            {detail.data.plans?.map((p) => (
              <Card key={p.id} title={p.device_name}>
                <p>
                  {!p.delivery?.received_at
                    ? '等待投递'
                    : (p.progress?.pause_reason ??
                      (p.progress?.next_eligible_at
                        ? `等待至 ${new Date(p.progress.next_eligible_at).toLocaleString()}`
                        : '手机已接收'))}
                </p>
                <ol>
                  {p.items?.map((item) => (
                    <li key={item.id}>
                      {item.snapshot.group?.name}：{states[item.state] ?? item.state}{' '}
                      <Button size="small" onClick={() => openTask(item.task_id)}>
                        查看发送账本 / 核对
                      </Button>
                    </li>
                  ))}
                </ol>
              </Card>
            ))}
            {detail.data.state === 'paused' && (
              <Card title="更换后续流程并重建剩余项">
                <Select
                  placeholder="选择已分配到执行设备的新流程"
                  value={releaseId}
                  onChange={setReleaseId}
                  options={releases.data?.items
                    .filter((r) => r.active && !r.revoked)
                    .map((r) => ({ value: r.id, label: r.name }))}
                />
                <p>
                  先在执行设备页更新流程分配。仅未取得许可且确定未发的群可以进入新预览，成功和
                  unknown 不会重发。
                </p>
                <Button
                  disabled={!releaseId}
                  loading={busy}
                  onClick={() =>
                    void run(async () => {
                      showPreview(
                        await api.dispatch.remaining(school, detail.data!, releaseId!, key()),
                      )
                      setDetailId('')
                    })
                  }
                >
                  生成剩余项核对预览
                </Button>
              </Card>
            )}
          </>
        )}
      </Drawer>
    </Space>
  )
}
