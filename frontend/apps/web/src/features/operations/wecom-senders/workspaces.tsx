import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Empty, Input, Modal, Select, Space, Table, Tag } from 'antd'
import { wecomWorkspaces, type WecomWorkspace, type WecomWorkspaceMember } from './api'

export function WecomWorkspacePanel({ platformAdmin, workspaces, selected, loading, error, onSelect, children, manage = false }: {
  platformAdmin: boolean; workspaces: WecomWorkspace[]; selected?: WecomWorkspace; loading: boolean; error: Error | null;
  onSelect: (id: string) => void; children: ReactNode; manage?: boolean
}) {
  const canManage = manage && platformAdmin
  const cache = useQueryClient()
  const [creating, setCreating] = useState<string>()
  const [name, setName] = useState('')
  const [membersOpen, setMembersOpen] = useState(false)
  const [username, setUsername] = useState('')
  const [role, setRole] = useState<'admin' | 'operator'>('operator')
  const members = useQuery({ queryKey: ['wecom-workspace-members', selected?.id], queryFn: () => wecomWorkspaces.members(selected!.id), enabled: membersOpen && !!selected && canManage })
  const save = useMutation({ mutationFn: () => wecomWorkspaces.save(creating!, name), onSuccess: async w => { await cache.invalidateQueries({ queryKey: ['wecom-workspaces'] }); onSelect(w.id); setCreating(undefined); setName('') } })
  const grant = useMutation({ mutationFn: (input: {username: string; role: 'admin' | 'operator' | 'revoked'}) => wecomWorkspaces.grant(selected!.id, input.username, input.role), onSuccess: async () => { setUsername(''); await cache.invalidateQueries({ queryKey: ['wecom-workspace-members'] }); await cache.invalidateQueries({ queryKey: ['wecom-workspaces'] }) } })
  return <Space orientation="vertical" style={{ width: '100%' }}>
    {manage && <>
      {canManage && <Button onClick={() => { setCreating(crypto.randomUUID()); setName('') }}>新建企业微信工作空间</Button>}
      <Table<WecomWorkspace> rowKey="id" loading={loading} dataSource={workspaces} pagination={false} locale={{ emptyText: '暂无工作空间，请新建企业微信工作空间。' }} columns={[
        { title: '工作空间', dataIndex: 'name' },
        { title: '当前使用', render: (_, w) => w.id === selected?.id ? <Tag color="blue">当前工作空间</Tag> : '—' },
        { title: '操作', render: (_, w) => <Space wrap>
          <Button disabled={w.id === selected?.id} onClick={() => onSelect(w.id)}>使用此工作空间</Button>
          {canManage && <Button onClick={() => { onSelect(w.id); setMembersOpen(true) }}>分发权限</Button>}
        </Space> },
      ]} />
    </>}
    {error && <Alert type="error" title={error.message} />}
    {manage ? children : selected ? children : <Empty description={loading ? '正在加载企业微信分发工作空间…' : '暂无企业微信分发授权，请联系系统管理员开通工作空间权限。'} />}
    {canManage && <><Modal title="新建企业微信工作空间" okText="确认创建" cancelText="取消" open={!!creating} confirmLoading={save.isPending} okButtonProps={{ disabled: !name.trim() }} onCancel={() => setCreating(undefined)} onOk={() => save.mutate()}>
      <Input aria-label="企业微信工作空间名称" value={name} maxLength={120} onChange={e => setName(e.target.value)} />
      {save.error && <Alert type="error" title={save.error.message} />}
    </Modal>
    <Modal title={`企业微信分发权限：${selected?.name ?? ''}`} open={membersOpen} footer={null} onCancel={() => setMembersOpen(false)} width={720}>
      <p>权限仅适用于当前企业微信分发工作空间。管理员可确认发送并管理目标群，运营可查看和准备任务。</p>
      <Space wrap><Input aria-label="授权登录账号" placeholder="已有登录账号的用户名" value={username} onChange={e => setUsername(e.target.value)} /><Select aria-label="企业微信分发角色" value={role} onChange={setRole} options={[{ value: 'operator', label: '运营' }, { value: 'admin', label: '管理员' }]} /><Button disabled={!username.trim()} loading={grant.isPending} onClick={() => grant.mutate({ username, role })}>保存分发授权</Button></Space>
      {(members.error || grant.error) && <Alert type="error" title={(members.error || grant.error)!.message} />}
      <Table<WecomWorkspaceMember> rowKey="id" dataSource={members.data?.items ?? []} loading={members.isFetching} columns={[{title:'账号',dataIndex:'username'},{title:'姓名',dataIndex:'display_name'},{title:'角色',render:(_,m)=>m.role==='admin'?'管理员':'运营'},{title:'操作',render:(_,m)=><Button danger loading={grant.isPending} onClick={()=>grant.mutate({username:m.username,role:'revoked'})}>撤销分发授权</Button>}]} />
    </Modal></>}
  </Space>
}
