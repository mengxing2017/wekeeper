import { Input, Modal } from 'antd'

export function useDialogs() {
  const [modal, contextHolder] = Modal.useModal()
  async function prompt(title: string) {
    let value = ''
    const accepted = await modal.confirm({ title, icon: null, okText: '确认', cancelText: '取消',
      content: <Input.TextArea autoFocus aria-label={title} onChange={event => { value = event.target.value }} /> })
    return accepted ? value : null
  }
  async function confirm(title: string) {
    return Boolean(await modal.confirm({ title, okText: '确认', cancelText: '取消' }))
  }
  return { prompt, confirm, contextHolder }
}
