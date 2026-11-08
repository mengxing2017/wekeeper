import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Input, Modal, Space, Switch, Table, Tag } from 'antd'
import { identity, type Administrator, type User } from './api'

export function Administrators({ user }: { user: User }) {
  const cache = useQueryClient()
  const [search, setSearch] = useState('')
  const [keyword, setKeyword] = useState('')
  const query = useQuery({ queryKey: ['administrators', search], queryFn: () => identity.administrators(search) })
  const [editor, setEditor] = useState<{ row?: Administrator }>()
  const [username, setUsername] = useState('')
  const [name, setName] = useState('')
  const [active, setActive] = useState(true)
  const [initialPassword, setInitialPassword] = useState('')
  const [deleting, setDeleting] = useState<Administrator>()
  const [selected, setSelected] = useState<Administrator>()
  const [password, setPassword] = useState('')
  const [notice, setNotice] = useState('')
  const refresh = () => cache.invalidateQueries({ queryKey: ['administrators'] })
  const save = useMutation({
    mutationFn: () => editor?.row
      ? identity.updateAdministrator(editor.row.id, { display_name: name.trim(), active })
      : identity.createAdministrator({ username: username.trim(), display_name: name.trim(), password: initialPassword }),
    onSuccess: async () => {
      const current = editor?.row?.id === user.id
      setNotice(editor?.row ? '管理员已更新' : '管理员已新增')
      setEditor(undefined); setInitialPassword('')
      await refresh()
      if (current) await cache.invalidateQueries({ queryKey: ['session'] })
    },
  })
  const remove = useMutation({
    mutationFn: () => identity.deleteAdministrator(deleting!.id),
    onSuccess: async () => { setDeleting(undefined); setNotice('管理员已删除，该账号已停用'); await refresh() },
  })
  const reset = useMutation({ mutationFn: () => identity.resetAdministrator(selected!.id, password), onSuccess: async () => {
    const current = selected?.id === user.id
    setSelected(undefined); setPassword(''); setNotice('密码已重设，该账号的现有会话已失效')
    if (current) {
      await cache.cancelQueries()
      cache.setQueryData(['session'], null)
      cache.removeQueries({ predicate: query => query.queryKey[0] !== 'session' })
    }
    else await refresh()
  } })
  const openEditor = (row?: Administrator) => {
    save.reset(); setEditor({ row }); setUsername(row?.username ?? ''); setName(row?.display_name ?? '')
    setActive(row?.active ?? true); setInitialPassword('')
  }
  return <Card title="系统管理员">
    <Space orientation="vertical" style={{ width: '100%' }}>
      <p>维护系统管理员账号、姓名和启用状态。停用或删除后该账号不能登录，现有会话立即失效。</p>
      <Space wrap>
        <Input.Search aria-label="查询管理员" placeholder="按账号或姓名查询" value={keyword} maxLength={100} allowClear enterButton="查询" onChange={e => setKeyword(e.target.value)} onSearch={value => setSearch(value.trim())} />
        <Button onClick={() => { setKeyword(''); setSearch('') }}>重置查询</Button>
        <Button type="primary" onClick={() => openEditor()}>新增管理员</Button>
      </Space>
      {notice && <Alert type="success" title={notice} closable onClose={() => setNotice('')} />}
      {query.error && <Alert type="error" title={query.error.message} />}
      <Table<Administrator> rowKey="id" dataSource={query.data ?? []} loading={query.isFetching} locale={{ emptyText: '暂无匹配的管理员' }} columns={[
        { title: '账号', dataIndex: 'username' }, { title: '姓名', dataIndex: 'display_name' },
        { title: '状态', render: (_, row) => <Tag>{row.active ? '启用' : '停用'}</Tag> },
        { title: '操作', render: (_, row) => <Space wrap>
          <Button onClick={() => openEditor(row)}>编辑</Button>
          <Button onClick={() => { reset.reset(); setSelected(row); setPassword('') }}>重设密码</Button>
          <Button danger disabled={row.id === user.id} onClick={() => { remove.reset(); setDeleting(row) }}>删除</Button>
        </Space> },
      ]} />
    </Space>
    <Modal title={editor?.row ? '编辑管理员' : '新增管理员'} okText="保存管理员" cancelText="取消" open={!!editor} confirmLoading={save.isPending} okButtonProps={{ disabled: !username.trim() || !name.trim() || (!editor?.row && new TextEncoder().encode(initialPassword).length < 12) }} onCancel={() => { setEditor(undefined); setInitialPassword('') }} onOk={() => save.mutate()}>
      <Space orientation="vertical" style={{ width: '100%' }}>
        <label>账号<Input aria-label="管理员账号" autoComplete="off" maxLength={100} disabled={!!editor?.row} value={username} onChange={e => setUsername(e.target.value)} /></label>
        <label>姓名<Input aria-label="管理员姓名" maxLength={100} value={name} onChange={e => setName(e.target.value)} /></label>
        {editor?.row ? <label>启用状态 <Switch aria-label="管理员启用状态" checked={active} disabled={editor.row.id === user.id} onChange={setActive} /></label> : <label>初始密码<Input.Password aria-label="管理员初始密码" autoComplete="new-password" maxLength={128} value={initialPassword} onChange={e => setInitialPassword(e.target.value)} /><span>密码须为 12–128 字节。</span></label>}
        {save.error && <Alert type="error" title={save.error.message} />}
      </Space>
    </Modal>
    <Modal title="删除管理员" okText="确认删除管理员" cancelText="取消" open={!!deleting} confirmLoading={remove.isPending} okButtonProps={{ danger: true }} onCancel={() => setDeleting(undefined)} onOk={() => remove.mutate()}>
      <p>确认删除管理员“{deleting?.username}”？删除后撤销管理员身份、停用账号并注销所有会话，历史任务和审计记录保留。</p>
      {remove.error && <Alert type="error" title={remove.error.message} />}
    </Modal>
    <Modal title="重设管理员密码" okText="确认重设密码" cancelText="取消" open={!!selected} confirmLoading={reset.isPending} okButtonProps={{ disabled: password.length < 12 }} onCancel={() => setSelected(undefined)} onOk={() => reset.mutate()}>
      <Input.Password aria-label="新密码" autoComplete="new-password" maxLength={128} value={password} onChange={e => setPassword(e.target.value)} />
      {reset.error && <Alert type="error" title={reset.error.message} />}
    </Modal>
  </Card>
}
