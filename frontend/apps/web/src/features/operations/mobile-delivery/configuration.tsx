import { createIdempotencyKey as key } from '../../../shared/idempotency-key'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Input, Select, Space, Table } from 'antd'
import { Editor } from '../../admin/workspace/editor'
import { PaginationControls } from '../../../shared/pagination-controls'
import {
  api,
  type ConfigurationInput,
  type ExecutionConfiguration,
  type ConfigurationRelease,
} from './api'
/** Expert-only editor. Rules are immutable data; publishing never launches a client or sends. */
export function ConfigurationWorkbench() {
  const [page, setPage] = useState(1),
    [releasePage, setReleasePage] = useState(1),
    [source, setSource] = useState<ExecutionConfiguration>(),
    [document, setDocument] = useState(''),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const cache = useQueryClient()
  const versions = useQuery({
    queryKey: ['mobile', 'platform', 'configurations', page],
    queryFn: () => api.configurations(page),
  })
  const releases = useQuery({
    queryKey: ['mobile', 'platform', 'releases', releasePage],
    queryFn: () => api.configurationReleases(releasePage),
  })
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['mobile', 'platform'] })
  }
  const save = async () => {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      const input = JSON.parse(document) as ConfigurationInput
      const result = await api.createConfiguration(input, key())
      setSource(result)
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <Space orientation="vertical" className="full-width">
      <Alert
        type="info"
        title="规则包不可变；修改时创建新版本，发布到学校或指定手机。撤销立即阻断后续资格，旧快照和结果保留。"
        description="根据真实客户端版本及非 ID 节点样本配置页面、控件、导航图、人工基准和执行政策；不使用企业微信业务 ID、控件 ID、坐标或脚本。界面升级后发布新配置，能力边界不变时无需更新 APK。"
      />
      {[versions.error, releases.error].map(
        (e, i) => e && <Alert key={i} type="error" title={e.message} />,
      )}
      {error && <Alert type="error" title={error} />}
      <Card title="规则版本与新版本编辑">
        <Select
          aria-label="复制规则版本"
          placeholder="选择已有版本进行复制、比较或回退"
          style={{ minWidth: 400 }}
          value={source?.id}
          options={versions.data?.items.map((v) => ({ value: v.id, label: `${v.name} · ${v.id}` }))}
          onChange={(id) => {
            const v = versions.data?.items.find((v) => v.id === id)
            setSource(v)
            setDocument(v ? JSON.stringify(v.configuration, null, 2) : '')
          }}
        />
        <p>
          完整配置 JSON 包含
          name、rules、policy、baseline、evidence_ref。所有时长使用毫秒；提交时服务端校验
          schema、路径、能力、人工样本及时间下限。未提供真实样本时不生成默认企业微信规则。
        </p>
        <Input.TextArea
          aria-label="完整规则与政策 JSON"
          rows={18}
          value={document}
          onChange={(e) => setDocument(e.target.value)}
        />
        {source && (
          <details>
            <summary>原版本内容（对照新配置） · {source.content_digest}</summary>
            <pre>{JSON.stringify(source.configuration, null, 2)}</pre>
          </details>
        )}
        <Button type="primary" loading={busy} onClick={() => void save()}>
          校验并保存不可变新版本
        </Button>
        <PaginationControls
          page={page}
          more={versions.data?.page?.has_more}
          change={setPage}
          subject="配置版本"
        />
        {source && (
          <Editor
            key={source.id}
            submit="发布此版本（回退时也创建新发布记录）"
            fields={[
              { name: 'school_id', title: '发布学校 ID（本系统）' },
              { name: 'device_id', title: '灰度手机 ID（本系统，留空覆盖学校）', optional: true },
              { name: 'evidence_ref', title: '只读试运行、人工基准和发布核对依据' },
            ]}
            onSave={async (v) => {
              await api.publishConfiguration(
                source.id,
                {
                  expected_revision: source.revision,
                  school_id: v.school_id,
                  device_id: v.device_id || null,
                  evidence_ref: v.evidence_ref,
                },
                key(),
              )
              await refresh()
            }}
          />
        )}
      </Card>
      <Card title="发布历史与撤销">
        <p>
          只读验证：复制下方与选定规则版本对应的发布 JSON，在 Otter
          调试版只读采样页面粘贴；逐页人工打开页面后采样，核对页面分类及角色匹配数量。该入口不导航、不输入、不发送。
        </p>
        {releases.data?.items
          .filter((r) => r.configuration_id === source?.id)
          .map((r) => (
            <details key={r.id}>
              <summary>只读试运行配置 · {r.id}</summary>
              <pre>
                {JSON.stringify(
                  {
                    ...r,
                    configuration: source!.configuration,
                    content_digest: source!.content_digest,
                  },
                  null,
                  2,
                )}
              </pre>
            </details>
          ))}
        <Table<ConfigurationRelease>
          rowKey="id"
          pagination={false}
          dataSource={releases.data?.items ?? []}
          columns={[
            { title: '规则', dataIndex: 'name' },
            { title: '学校', dataIndex: 'school_id' },
            { title: '手机（空为学校）', dataIndex: 'device_id' },
            {
              title: '状态',
              render: (_, r) => (r.revoked ? '已撤销' : r.active ? '生效' : '已被新发布替代'),
            },
            {
              title: '撤销',
              render: (_, r) =>
                !r.revoked && (
                  <Editor
                    key={r.revision}
                    submit="撤销发布"
                    fields={[{ name: 'reason', title: '撤销原因' }]}
                    onSave={async (v) => {
                      await api.revokeConfigurationRelease(
                        r.id,
                        { expected_revision: r.revision, reason: v.reason },
                        key(),
                      )
                      await refresh()
                    }}
                  />
                ),
            },
          ]}
        />
        <PaginationControls
          page={releasePage}
          more={releases.data?.page?.has_more}
          change={setReleasePage}
          subject="发布记录"
        />
      </Card>
    </Space>
  )
}
