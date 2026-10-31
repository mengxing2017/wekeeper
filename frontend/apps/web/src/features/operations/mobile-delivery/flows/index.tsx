import { useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Checkbox, Input, InputNumber, Select, Space, Steps } from 'antd'
import { api, type AutomationPreview } from '../api'
import { createIdempotencyKey as key } from '../../../../shared/idempotency-key'
import { PaginationControls } from '../../../../shared/pagination-controls'
export function BusinessWizard({
  school,
  openTask,
}: {
  school: string
  openTask: (id: string) => void
}) {
  const [step, setStep] = useState(0),
    [bindingId, setBindingId] = useState(''),
    [name, setName] = useState('企业微信群发送'),
    [text, setText] = useState(''),
    [groups, setGroups] = useState<string[]>([]),
    [interval, setInterval] = useState<number | null>(null),
    [unit, setUnit] = useState(60),
    [reviewed, setReviewed] = useState(false),
    [preview, setPreview] = useState<AutomationPreview>(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [page, setPage] = useState(1),
    [groupPage, setGroupPage] = useState(1)
  const lock = useRef(false),
    confirmKey = useRef('')
  const bindings = useQuery({
    queryKey: ['mobile', school, 'app-bindings', page],
    queryFn: () => api.automation.bindings(school, page),
  })
  const apps = useQuery({
    queryKey: ['mobile', school, 'automation-apps'],
    queryFn: () => api.automation.apps(school),
  })
  const groupList = useQuery({
    queryKey: ['mobile', school, 'wizard-groups', groupPage],
    queryFn: () => api.groups(school, groupPage, 100),
  })
  const selectedBinding =
    bindingId || (bindings.data?.items.length === 1 ? bindings.data.items[0].id : '')
  const binding = bindings.data?.items.find((b) => b.id === selectedBinding)
  const app = apps.data?.items.find((a) => a.id === binding?.app_id)
  const capability = app?.capabilities[0]
  const resolution = useQuery({
    queryKey: ['mobile', school, 'capability-resolution', selectedBinding],
    queryFn: () => api.automation.resolution(school, selectedBinding),
    enabled: !!selectedBinding,
    retry: false,
  })
  const policy = resolution.data?.configuration.policy
  const baseline = resolution.data?.configuration.baseline
  const minimum = Math.ceil(
    Math.max(policy?.minimum_group_interval_ms ?? 0, baseline?.human_group_min_interval_ms ?? 0) /
      1000,
  )
  const seconds =
    interval === null ? Math.ceil((policy?.default_group_interval_ms ?? 0) / 1000) : interval * unit
  const invalidate = () => {
    setPreview(undefined)
    setReviewed(false)
    confirmKey.current = ''
    setError('')
  }
  async function perform(action: () => Promise<void>) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      await action()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  const groupNames = useRef(new Map<string, string>())
  for (const g of groupList.data?.items ?? []) groupNames.current.set(g.id, g.name)
  function move(index: number, offset: number) {
    const copy = [...groups]
    ;[copy[index], copy[index + offset]] = [copy[index + offset], copy[index]]
    invalidate()
    setGroups(copy)
  }
  return (
    <Space orientation="vertical" className="full-width">
      <Steps
        current={step}
        items={[
          { title: '应用与业务' },
          { title: '内容与目标' },
          { title: '节奏与安排' },
          { title: '预览确认' },
        ]}
      />
      {[bindings.error, apps.error, groupList.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {error && <Alert type="error" title={error} />}
      {step === 0 && (
        <Card title="选择已设置的应用与账号">
          <Space orientation="vertical">
            <Select
              aria-label="业务手机与应用"
              style={{ minWidth: 320 }}
              placeholder="先在手机与应用完成三步设置"
              value={selectedBinding || undefined}
              options={bindings.data?.items.map((b) => ({
                value: b.id,
                label: `${b.device_name} · ${b.app_name} · ${b.account_name}`,
              }))}
              onChange={(v) => {
                invalidate()
                setBindingId(v)
                setInterval(null)
              }}
            />
            <PaginationControls
              page={page}
              more={bindings.data?.page?.has_more}
              change={(v) => {
                invalidate()
                setPage(v)
                setBindingId('')
              }}
              subject="业务手机与应用"
            />
            <p>业务：{capability?.name ?? '等待选择受支持应用'}。当前仅支持手动运行一次。</p>
            {resolution.error && (
              <Alert
                type="warning"
                title="应用尚未准备好"
                description={`${resolution.error.message} 请先检查“手机与应用”，或联系适配维护人员完成真实环境验证。`}
              />
            )}
            <Button
              loading={resolution.isFetching}
              disabled={!selectedBinding}
              onClick={() => void resolution.refetch()}
            >
              重新检查适配状态
            </Button>
            <Button
              type="primary"
              disabled={
                !capability || !resolution.data || resolution.isError || resolution.isFetching
              }
              onClick={() => setStep(1)}
            >
              下一步：内容与目标
            </Button>
          </Space>
        </Card>
      )}
      {step === 1 && (
        <Card title="正文和群发送顺序">
          <Space orientation="vertical" className="full-width">
            <label>
              业务名称
              <Input
                aria-label="业务名称"
                value={name}
                onChange={(e) => {
                  invalidate()
                  setName(e.target.value)
                }}
              />
            </label>
            <label>
              消息正文
              <Input.TextArea
                aria-label="业务消息正文"
                value={text}
                rows={5}
                onChange={(e) => {
                  invalidate()
                  setText(e.target.value)
                }}
              />
            </label>
            <Select
              mode="multiple"
              optionFilterProp="label"
              aria-label="业务目标群"
              placeholder="按发送顺序选择已核对目标群"
              className="full-width"
              value={groups}
              options={groupList.data?.items
                .filter(
                  (g) =>
                    g.enabled &&
                    g.identity_mode === 'ui_visible' &&
                    g.verification_state === 'reviewed',
                )
                .map((g) => ({ value: g.id, label: g.name }))}
              onChange={(v) => {
                invalidate()
                setGroups(v)
              }}
            />
            <PaginationControls
              page={groupPage}
              more={groupList.data?.page?.has_more}
              change={setGroupPage}
              subject="业务目标群"
            />
            <ol>
              {groups.map((g, i) => (
                <li key={g}>
                  {groupNames.current.get(g) || '已选择的目标群'}{' '}
                  <Button size="small" disabled={i === 0} onClick={() => move(i, -1)}>
                    上移
                  </Button>{' '}
                  <Button
                    size="small"
                    disabled={i === groups.length - 1}
                    onClick={() => move(i, 1)}
                  >
                    下移
                  </Button>
                </li>
              ))}
            </ol>
            <Space>
              <Button onClick={() => setStep(0)}>上一步</Button>
              <Button
                type="primary"
                disabled={!text.trim() || !name.trim() || groups.length === 0}
                onClick={() => setStep(2)}
              >
                下一步：节奏与安排
              </Button>
            </Space>
          </Space>
        </Card>
      )}
      {step === 2 && (
        <Card title="群间等待">
          <Space orientation="vertical">
            <p>
              建议 {Math.ceil((policy?.default_group_interval_ms ?? 0) / 1000)} 秒，至少 {minimum}{' '}
              秒。前一群完成并确认结果后，等待结束才查找下一群。
            </p>
            <Space>
              <InputNumber
                aria-label="群间等待数值"
                min={1}
                max={31536000}
                precision={0}
                value={interval === null ? seconds / unit : interval}
                onChange={(v) => {
                  invalidate()
                  setInterval(v)
                }}
              />
              <Select
                aria-label="群间等待单位"
                value={unit}
                options={[
                  { value: 1, label: '秒' },
                  { value: 60, label: '分钟' },
                ]}
                onChange={(v) => {
                  invalidate()
                  setUnit(v)
                  setInterval(Math.ceil(seconds / v))
                }}
              />
            </Space>
            <p>
              群间等待合计至少 {Math.max(0, groups.length - 1) * seconds}{' '}
              秒；另受账号冷却、额度、手机会话和实际操作耗时约束。
            </p>
            <p>安排：手动运行一次。浏览器确认后，仍需在手机启动有效会话。</p>
            <Space>
              <Button onClick={() => setStep(1)}>上一步</Button>
              <Button
                type="primary"
                loading={busy}
                disabled={
                  !capability ||
                  !Number.isSafeInteger(seconds) ||
                  seconds < minimum ||
                  seconds < 1 ||
                  seconds > 31536000
                }
                onClick={() =>
                  void perform(async () => {
                    if (!capability) return
                    const p = await api.automation.preview(
                      school,
                      {
                        app_binding_id: selectedBinding,
                        capability_id: capability.id,
                        name,
                        group_ids: groups,
                        text_content: text,
                        interval_seconds: seconds,
                        schedule: 'manual_once',
                      },
                      key(),
                    )
                    setPreview(p)
                    confirmKey.current = key()
                    setReviewed(false)
                    setStep(3)
                  })
                }
              >
                生成服务端预览
              </Button>
            </Space>
          </Space>
        </Card>
      )}
      {step === 3 && preview && (
        <Card title="复核并确认">
          <Space orientation="vertical">
            <p>
              {binding?.device_name} · {binding?.app_name} · {binding?.account_name}
            </p>
            <p>
              按上述群顺序逐个发送，每群间等待 {preview.input.interval_seconds} 秒。预览有效至{' '}
              {new Date(preview.expires_at).toLocaleString()}。
            </p>
            <ol>
              {groups.map((g) => (
                <li key={g}>{groupNames.current.get(g) || '已选择的目标群'}</li>
              ))}
            </ol>
            <pre style={{ whiteSpace: 'pre-wrap' }}>{preview.input.text_content}</pre>
            <Checkbox checked={reviewed} onChange={(e) => setReviewed(e.target.checked)}>
              已逐群复核账号、手机、目标顺序、正文和同一正文发送范围
            </Checkbox>
            <Alert
              type="info"
              title="确认后等待手机启动"
              description="环境、账号绑定或适配发布变化会使预览失效。结果不确定时须人工核对，后续群停止。"
            />
            <Space>
              <Button
                disabled={busy}
                onClick={() => {
                  invalidate()
                  setStep(2)
                }}
              >
                返回修改
              </Button>
              <Button
                type="primary"
                loading={busy}
                disabled={!reviewed || Date.parse(preview.expires_at) <= Date.now()}
                onClick={() =>
                  void perform(async () => {
                    const run = await api.automation.confirm(school, preview, confirmKey.current)
                    invalidate()
                    openTask(run.task_id)
                  })
                }
              >
                确认本次业务
              </Button>
            </Space>
          </Space>
        </Card>
      )}
    </Space>
  )
}
