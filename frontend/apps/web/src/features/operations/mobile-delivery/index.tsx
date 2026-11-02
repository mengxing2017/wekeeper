import { ControlStatus, useControlFreshness } from './control-status'
import { DeviceControlPanel } from './device-control'
import { createIdempotencyKey as key } from '../../../shared/idempotency-key'
import { DeviceStateEditor } from './device-state'
import { UsageManagement } from '../../admin/mobile-resources/usage-management'
import { ActionDrawer } from '../../../shared/action-drawer'
import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Drawer,
  Table,
  Space,
  Tabs,
  Tag,
  Typography,
} from 'antd'
import { Editor } from '../../admin/workspace/editor'
import { PaginationControls } from '../../../shared/pagination-controls'
import { api } from './api'


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

export function ExecutionDevices({ view, onViewChange }: { view?: string; onViewChange?: (view: string) => void } = {}) {
  const [internalTab, setInternalTab] = useState('devices')
  const listTab = view ?? internalTab
  const setListTab = (value: string) => onViewChange ? onViewChange(value) : setInternalTab(value)
  const [page, setPage] = useState(1),
    [pairing, setPairing] = useState<{ code: string; expiresAt: string; name: string } | null>(
      null,
    ),
    [deviceId, setDeviceId] = useState('')
  const devices = useQuery({
    queryKey: ['mobile', 'platform', 'devices', page],
    queryFn: () => api.devices(page),
    enabled: listTab === 'devices',
    refetchInterval: listTab === 'devices' ? 5000 : false,
    refetchOnWindowFocus: 'always',
  })
  const devicesFresh = useControlFreshness(devices.dataUpdatedAt, devices.isError)
  const cache = useQueryClient(),
    actions = useActions('platform')
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', 'platform'] })
  }
  return (
    <section className="management-stack">
      {[devices.error].map(
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

      <Tabs activeKey={listTab} onChange={setListTab} items={[
        { key: 'devices', label: '手机设备' },
        { key: 'usage', label: '业务使用开通' },
      ]} />
      {listTab === 'usage' ? (
        <UsageManagement />
      ) : (
        <>
          <Card
            className="data-panel"
            title="手机设备列表"
            extra={
              <Space>
                <Button onClick={() => void refresh()}>刷新设备</Button>
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
                  title: '启用状态',
                  render: (_, d) => (
                    <Tag color={d.enabled ? 'success' : 'default'}>
                      {d.enabled ? '已启用' : '已停用'}
                    </Tag>
                  ),
                },
                { title: '控制连接', render: (_, d) => <ControlStatus state={d.control?.state} fresh={devicesFresh} /> },
                { title: '最近控制心跳', render: (_, d) => d.control?.last_control_heartbeat_at ?? '尚未收到心跳' },
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
                    设备 ID：{d.id} · 最近业务接触：{d.last_seen_at ?? '尚未连接'}
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
                  {!d.enabled && <Alert type="warning" title="此设备已停用"
                    description="可以配对，但连接验证和执行会被拒绝。请确认管理意图，需要恢复时在下方启用设备。" />}
                  <DeviceControlPanel deviceId={d.id} />
                  <DeviceStateEditor key={`state-${d.id}`} device={d} refresh={refresh} />

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
      )}
    </section>
  )
}
