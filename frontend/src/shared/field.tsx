import { Form } from 'antd'
import { cloneElement, useId, type ReactElement, type ReactNode } from 'react'

// Use the same label spacing for standalone controls and managed Form fields.
export function Field({ label, children, name, required = false }: { label: ReactNode; name?: string; required?: boolean; children: ReactElement<{ id?: string }> }) {
  const generatedId = useId()
  const id = children.props.id ?? generatedId
  return <Form.Item layout="vertical" label={label} htmlFor={id} name={name} rules={name ? [{ required, message: <>请填写{label}</> }] : undefined}>
    {cloneElement(children, { id })}
  </Form.Item>
}
