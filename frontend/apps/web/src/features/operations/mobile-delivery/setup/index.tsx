import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Select, Space, Steps, Tag } from 'antd'
import { api } from '../api'
import { createIdempotencyKey as key } from '../../../../shared/idempotency-key'
import { PaginationControls } from '../../../../shared/pagination-controls'
export const validationStates: Record<string, string> = {
  queued: '等待手机接受',
  offered: '等待手机接受',
  accepted: '手机已接受',
  running: '手机检查中',
  succeeded: '验证完成',
  failed: '验证失败，请查看手机',
  interrupted: '已中断，请重新发起',
  cancelled: '已取消（手机收到后停止）',
  expired: '已过期，请重新发起',
}
export function ApplicationSetup({ school }: { school: string }) {
  const [step, setStep] = useState(0),
    [device, setDevice] = useState(''),
    [appId, setAppId] = useState(''),
    [instance, setInstance] = useState(''),
    [page, setPage] = useState(1),
    [bindingPage, setBindingPage] = useState(1),
    [jobId, setJobId] = useState(''),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('')
  const targets = useQuery({
    queryKey: ['mobile', school, 'setup-targets', page],
    queryFn: () => api.targets(school, page),
  })
  const apps = useQuery({
    queryKey: ['mobile', school, 'automation-apps'],
    queryFn: () => api.automation.apps(school),
  })
  const bindings = useQuery({
    queryKey: ['mobile', school, 'app-bindings', bindingPage],
    queryFn: () => api.automation.bindings(school, bindingPage),
  })
  const selectedDevice =
    device || (targets.data?.devices.length === 1 ? targets.data.devices[0].id : '')
  const selectedApp = appId || apps.data?.items.find((a) => a.enabled)?.id || ''
  const applications = useQuery({
    queryKey: ['mobile', school, 'applications', selectedDevice],
    queryFn: () => api.automation.applications(school, selectedDevice),
    enabled: !!selectedDevice,
  })
  const job = useQuery({
    queryKey: ['mobile', school, 'validation', jobId],
    queryFn: () => api.automation.validation(school, jobId),
    enabled: !!jobId,
    refetchInterval: (q) =>
      q.state.data && ['queued', 'offered', 'accepted', 'running'].includes(q.state.data.state)
        ? 5000
        : false,
  })
  const environment = applications.data?.environment
  const report = environment?.body
  const app = apps.data?.items.find((a) => a.id === selectedApp)
  const candidates = report?.applications.filter((a) => a.app_key === app?.app_key) ?? []
  const chosenInstance = instance || (candidates.length === 1 ? candidates[0].instance_ref : '')
  const binding = bindings.data?.items.find(
    (b) => b.device_id === selectedDevice && b.app_id === selectedApp,
  )
  const checks = [
    ['新版手机端', !!report && report.engine_version === 'otter-automation/1'],
    ['通知权限', !!report?.notifications_ready],
    ['无障碍服务', !!report?.accessibility_ready],
    ['屏幕已解锁', !!report?.unlocked],
    [
      '唯一可见安装实例',
      candidates.some(
        (a) => a.instance_ref === chosenInstance && a.instance_visibility === 'unique',
      ),
    ],
  ] as const
  async function perform(action: () => Promise<void>) {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      await action()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const clear = () => {
    setStep(0)
    setInstance('')
    setJobId('')
    setError('')
  }
  return (
    <Space orientation="vertical" className="full-width">
      <Steps
        current={step}
        items={[{ title: '选择手机和应用' }, { title: '检查手机' }, { title: '测试打开' }]}
      />
      {[targets.error, apps.error, bindings.error, applications.error, job.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {error && <Alert type="error" title={error} />}
      {step === 0 && (
        <Card title="选择手机和应用">
          <Space orientation="vertical">
            <Select
              aria-label="设置手机"
              placeholder="选择已配对并绑定账号的手机"
              style={{ minWidth: 300 }}
              value={selectedDevice || undefined}
              options={targets.data?.devices.map((d) => ({
                value: d.id,
                label: d.name,
                disabled: !d.enabled,
              }))}
              onChange={(v) => {
                clear()
                setDevice(v)
              }}
            />
            <PaginationControls
              page={page}
              more={targets.data?.devices_has_more}
              change={(v) => {
                clear()
                setPage(v)
              }}
              subject="可设置手机"
            />
            <Select
              aria-label="设置应用"
              style={{ minWidth: 300 }}
              value={selectedApp || undefined}
              options={apps.data?.items.map((a) => ({
                value: a.id,
                label: a.enabled ? a.name : `${a.name} · 尚未支持`,
                disabled: !a.enabled,
              }))}
              onChange={(v) => {
                clear()
                setAppId(v)
              }}
            />
            <p>
              手机由平台登记、短码配对并绑定已核对账号。只检查目录内的应用，不读取整机应用清单。
            </p>
            <Button
              type="primary"
              disabled={!selectedDevice || !selectedApp}
              onClick={() => setStep(1)}
            >
              下一步：检查手机
            </Button>
          </Space>
        </Card>
      )}
      {step === 1 && (
        <Card title="检查环境与权限">
          <Space orientation="vertical">
            {!report && (
              <Alert
                type="info"
                title="等待手机检查"
                description={
                  applications.data?.next_step || '等待支持 v4 的新版 Otter 上报手机环境。'
                }
              />
            )}
            {checks.map(([label, ready]) => (
              <div key={label}>
                <Tag color={ready ? 'success' : 'warning'}>{ready ? '已就绪' : '待处理'}</Tag>
                {label}
              </div>
            ))}
            {report && (
              <Select
                aria-label="已安装应用实例"
                placeholder="选择手机返回的安装实例"
                style={{ minWidth: 300 }}
                value={chosenInstance || undefined}
                options={candidates.map((a) => ({
                  value: a.instance_ref,
                  label: `${a.label} · ${a.client.version_name}${a.instance_visibility !== 'unique' ? ' · 无法区分实例' : ''}`,
                  disabled: a.instance_visibility !== 'unique',
                }))}
                onChange={setInstance}
              />
            )}
            <Button loading={applications.isFetching} onClick={() => void applications.refetch()}>
              刷新检查结果
            </Button>
            <Button
              disabled={
                !targets.data?.devices
                  .find((d) => d.id === selectedDevice)
                  ?.supported_protocol_versions?.includes(4) || busy
              }
              onClick={() =>
                void perform(async () => {
                  const j = await api.automation.validate(
                    school,
                    {
                      device_id: selectedDevice,
                      app_binding_id: null,
                      kind: 'inspect_environment',
                      component_ids: [],
                    },
                    key(),
                  )
                  setJobId(j.id)
                })
              }
            >
              请手机重新检测
            </Button>
            {job.data && <p role="status">{validationStates[job.data.state]}</p>}
            <Space>
              <Button onClick={() => setStep(0)}>上一步</Button>
              <Button
                type="primary"
                loading={busy}
                disabled={!checks.every(([, ready]) => ready)}
                onClick={() =>
                  void perform(async () => {
                    if (!environment) return
                    if (
                      !binding ||
                      binding.environment_id !== environment.id ||
                      binding.instance_ref !== chosenInstance
                    ) {
                      await api.automation.bind(
                        school,
                        {
                          device_id: selectedDevice,
                          app_id: selectedApp,
                          environment_id: environment.id,
                          instance_ref: chosenInstance,
                          expected_revision: binding?.revision ?? 0,
                        },
                        key(),
                      )
                      await bindings.refetch()
                    }
                    setJobId('')
                    setStep(2)
                  })
                }
              >
                下一步：测试打开
              </Button>
            </Space>
            <PaginationControls
              page={bindingPage}
              more={bindings.data?.page?.has_more}
              change={setBindingPage}
              subject="已设置应用"
            />
          </Space>
        </Card>
      )}
      {step === 2 && (
        <Card title="仅测试打开应用">
          <Space orientation="vertical">
            <Alert
              type="info"
              title="手机需要明确接受测试"
              description="测试仅通过桌面搜索或全部应用列表打开并核验企业微信，不搜索群、不输入消息、不发送。尚无适配时联系维护人员完成试用。"
            />
            <Button
              type="primary"
              loading={busy}
              disabled={
                !binding ||
                (!!job.data &&
                  ['queued', 'offered', 'accepted', 'running'].includes(job.data.state))
              }
              onClick={() =>
                void perform(async () => {
                  if (!binding) return
                  const j = await api.automation.validate(
                    school,
                    {
                      device_id: selectedDevice,
                      app_binding_id: binding.id,
                      kind: 'open_application',
                      component_ids: [],
                    },
                    key(),
                  )
                  setJobId(j.id)
                })
              }
            >
              发起测试打开
            </Button>
            {job.data && (
              <>
                <p role="status">{validationStates[job.data.state]}</p>
                {['queued', 'offered', 'accepted', 'running'].includes(job.data.state) && (
                  <Button
                    loading={busy}
                    onClick={() =>
                      void perform(async () => {
                        await api.automation.cancel(school, job.data.id, job.data.revision, key())
                        await job.refetch()
                      })
                    }
                  >
                    取消测试
                  </Button>
                )}
              </>
            )}
            <Button onClick={() => setStep(1)}>返回检查</Button>
          </Space>
        </Card>
      )}
    </Space>
  )
}
