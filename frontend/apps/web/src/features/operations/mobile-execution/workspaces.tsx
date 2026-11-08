import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Empty, Input, Modal, Select, Space, Table } from 'antd'
import { mobileWorkspaces, type MobileWorkspace, type MobileWorkspaceMember } from './api'

export function MobileWorkspacePanel({ platformAdmin, workspaces, selected, loading, error, onSelect, children }: {
  platformAdmin: boolean; workspaces: MobileWorkspace[]; selected?: MobileWorkspace; loading: boolean; error: Error | null;
  onSelect: (id: string) => void; children: ReactNode
}) {
  const cache = useQueryClient()
  const [creating, setCreating] = useState<string>()
  const [name, setName] = useState('')
  const [membersOpen, setMembersOpen] = useState(false)
  const [username, setUsername] = useState('')
  const [role, setRole] = useState<'admin' | 'operator'>('operator')
  const members = useQuery({ queryKey: ['mobile-workspace-members', selected?.id], queryFn: () => mobileWorkspaces.members(selected!.id), enabled: membersOpen && !!selected && platformAdmin })
  const save = useMutation({ mutationFn: () => mobileWorkspaces.save(creating!, name), onSuccess: async w => { await cache.invalidateQueries({ queryKey: ['mobile-workspaces'] }); onSelect(w.id); setCreating(undefined); setName('') } })
  const grant = useMutation({ mutationFn: (input: {username: string; role: 'admin' | 'operator' | 'revoked'}) => mobileWorkspaces.grant(selected!.id, input.username, input.role), onSuccess: async () => { setUsername(''); await cache.invalidateQueries({ queryKey: ['mobile-workspace-members'] }); await cache.invalidateQueries({ queryKey: ['mobile-workspaces'] }) } })
  return <Space orientation="vertical" style={{ width: '100%' }}>
    <Space wrap>
      <Select aria-label="移动分发工作空间" placeholder="选择移动分发工作空间" loading={loading} value={selected?.id} style={{ minWidth: 220 }} options={workspaces.map(w => ({ value: w.id, label: w.name }))} onChange={onSelect} />
      {platformAdmin && <><Button onClick={() => { setCreating(crypto.randomUUID()); setName('') }}>新建移动工作空间</Button><Button disabled={!selected} onClick={() => setMembersOpen(true)}>移动分发权限</Button></>}
    </Space>
    {error && <Alert type="error" title={error.message} />}
    {selected ? children : <Empty description={loading ? '正在加载移动分发工作空间…' : '暂无移动分发授权，请联系系统管理员开通工作空间权限。'} />}
    <Modal title="新建移动工作空间" open={!!creating} confirmLoading={save.isPending} okButtonProps={{ disabled: !name.trim() }} onCancel={() => setCreating(undefined)} onOk={() => save.mutate()}>
      <Input aria-label="移动工作空间名称" value={name} maxLength={120} onChange={e => setName(e.target.value)} />
      {save.error && <Alert type="error" title={save.error.message} />}
    </Modal>
    <Modal title="移动分发权限" open={membersOpen} footer={null} onCancel={() => setMembersOpen(false)} width={720}>
      <p>权限仅适用于当前移动分发工作空间。管理员可确认发送并管理目标群，运营可查看和准备任务。</p>
      <Space wrap><Input aria-label="授权登录账号" placeholder="已有登录账号的用户名" value={username} onChange={e => setUsername(e.target.value)} /><Select aria-label="移动分发角色" value={role} onChange={setRole} options={[{ value: 'operator', label: '运营' }, { value: 'admin', label: '管理员' }]} /><Button disabled={!username.trim()} loading={grant.isPending} onClick={() => grant.mutate({ username, role })}>保存移动授权</Button></Space>
      {(members.error || grant.error) && <Alert type="error" title={(members.error || grant.error)!.message} />}
      <Table<MobileWorkspaceMember> rowKey="id" dataSource={members.data?.items ?? []} loading={members.isFetching} columns={[{title:'账号',dataIndex:'username'},{title:'姓名',dataIndex:'display_name'},{title:'角色',render:(_,m)=>m.role==='admin'?'管理员':'运营'},{title:'操作',render:(_,m)=><Button danger loading={grant.isPending} onClick={()=>grant.mutate({username:m.username,role:'revoked'})}>撤销移动授权</Button>}]} />
    </Modal>
  </Space>
}
