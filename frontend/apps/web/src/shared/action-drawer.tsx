import { useState, type ReactNode } from 'react'
import { Button, Drawer } from 'antd'
import { PlusOutlined } from '@ant-design/icons'

/** Keep creation and configuration forms out of the list's reading flow. */
export function ActionDrawer({
  title,
  children,
  primary = false,
}: {
  title: string
  children: ReactNode
  primary?: boolean
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        type={primary ? 'primary' : 'default'}
        icon={primary ? <PlusOutlined /> : undefined}
        onClick={() => setOpen(true)}
      >
        {title}
      </Button>
      {open && (
        <Drawer open size="large" title={title} onClose={() => setOpen(false)}>
          {children}
        </Drawer>
      )}
    </>
  )
}
