import { Collapse } from 'antd'
import { useEffect, useState, type ReactNode } from 'react'

export function Disclosure({ title, open = false, children }: { title: ReactNode; open?: boolean; children: ReactNode }) {
  const [keys, setKeys] = useState<string[]>(open ? ['content'] : [])
  useEffect(() => { setKeys(open ? ['content'] : []) }, [open])
  return <Collapse className="disclosure" activeKey={keys} onChange={setKeys}
    items={[{ key: 'content', label: title, children }]} />
}
