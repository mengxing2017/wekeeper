import { useRef, useState } from 'react'
import { Button, Dropdown, Modal, type GetRef } from 'antd'

export interface TableRowAction {
  key: string
  label: string
  onClick?: () => unknown
  href?: string
  disabled?: boolean
  loading?: boolean
  danger?: boolean
  title?: string
  confirm?: { title: string; description?: string; okText?: string; cancelText?: string }
}

/** Keep the column budget independent of the number of available actions. */
export function TableRowActions({ actions, rowLabel, primaryCount }: {
  actions: (TableRowAction | false | null | undefined)[]
  rowLabel: string
  primaryCount?: number
}) {
  const [modal, contextHolder] = Modal.useModal()
  const [menuOpen, setMenuOpen] = useState(false)
  const trigger = useRef<GetRef<typeof Button>>(null)
  const visible = actions.filter((action): action is TableRowAction => Boolean(action))
  const count = primaryCount ?? (visible.length > 2 ? 1 : 2)
  const invoke = (action: TableRowAction) => {
    if (action.disabled || action.loading) return
    setMenuOpen(false)
    if (action.confirm) {
      modal.confirm({
        title: action.confirm.title,
        content: action.confirm.description,
        okText: action.confirm.okText ?? '确认',
        cancelText: action.confirm.cancelText ?? '取消',
        okButtonProps: { danger: action.danger },
        onOk: () => action.onClick?.(),
      })
    } else action.onClick?.()
  }
  const more = visible.slice(count)
  return <div className="table-row-actions">
    {visible.slice(0, count).map(action => <Button key={action.key} type="link"
      href={action.disabled || action.loading ? undefined : action.href}
      disabled={action.disabled} loading={action.loading} danger={action.danger} title={action.title}
      onClick={() => invoke(action)}>{action.label}</Button>)}
    {more.length > 0 && <Dropdown autoFocus open={menuOpen} onOpenChange={setMenuOpen} trigger={['click']} menu={{
      onKeyDown: event => {
        if (event.key === 'Escape') {
          // Close this menu without also dismissing its containing drawer/modal.
          event.stopPropagation()
          setMenuOpen(false)
          trigger.current?.focus()
        }
      },
      items: more.map(action => ({
      key: action.key,
      disabled: action.disabled || action.loading,
      danger: action.danger,
      label: action.href && !action.disabled && !action.loading
        ? <a href={action.href}>{action.label}</a> : <span title={action.title}>{action.label}</span>,
      onClick: () => invoke(action),
    })) }}>
      <Button ref={trigger} type="link" aria-expanded={menuOpen} aria-haspopup="menu" aria-label={`${rowLabel}的更多操作`}>更多</Button>
    </Dropdown>}
    {contextHolder}
  </div>
}
