import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { TableRowActions } from './table-row-actions'
import { TableIdentifier, TableText } from './table-text'

afterEach(cleanup)

it('keeps hidden and disabled actions from executing and preserves menu links', async () => {
  const edit = vi.fn(), disabled = vi.fn()
  render(<TableRowActions rowLabel="路由甲" actions={[
    {key:'main',label:'执行配置',onClick:edit},
    false,
    {key:'disabled',label:'编辑',disabled:true,onClick:disabled},
    {key:'receipts',label:'收件与回执',href:'/admin?view=source-receipts'},
  ]}/> )
  fireEvent.click(screen.getByRole('button',{name:'执行配置'}))
  expect(edit).toHaveBeenCalledTimes(1)
  fireEvent.click(screen.getByRole('button',{name:'路由甲的更多操作'}))
  const item = await screen.findByRole('menuitem',{name:'编辑'})
  expect(item.getAttribute('aria-disabled')).toBe('true')
  fireEvent.click(item)
  expect(disabled).not.toHaveBeenCalled()
  expect(screen.getByRole('link',{name:'收件与回执'}).getAttribute('href')).toBe('/admin?view=source-receipts')
})

it('requires confirmation after selecting a destructive menu action', async () => {
  const remove = vi.fn()
  render(<TableRowActions rowLabel="凭据甲" primaryCount={1} actions={[
    {key:'routes',label:'路由'},
    {key:'delete',label:'删除',danger:true,confirm:{title:'删除凭据？',description:'历史结果保留',okText:'确认删除'},onClick:remove},
  ]}/> )
  fireEvent.click(screen.getByRole('button',{name:/更多操作/}))
  fireEvent.click(await screen.findByRole('menuitem',{name:'删除'}))
  expect(await screen.findByText('历史结果保留')).toBeTruthy()
  expect(remove).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button',{name:'确认删除'}))
  await waitFor(()=>expect(remove).toHaveBeenCalledTimes(1))
})

it('closes only the menu on Escape and restores focus inside the containing view', async () => {
  const parentKey = vi.fn()
  render(<div onKeyDown={parentKey}><TableRowActions rowLabel="路由甲" primaryCount={1} actions={[
    {key:'main',label:'执行配置'}, {key:'edit',label:'编辑'},
  ]}/></div>)
  const trigger = screen.getByRole('button',{name:/更多操作/})
  fireEvent.click(trigger)
  const menu = await screen.findByRole('menu')
  fireEvent.keyDown(menu,{key:'Escape',code:'Escape'})
  expect(parentKey).not.toHaveBeenCalled()
  expect(document.activeElement).toBe(trigger)
  await waitFor(()=>expect(trigger.getAttribute('aria-expanded')).toBe('false'))
})

it('exposes full long text through native disclosure and copies the complete identifier', async () => {
  const value = 'identifier-完整标识-0123456789-abcdefghijklmnopqrstuvwxyz'
  const writeText = vi.fn().mockResolvedValue(undefined)
  vi.stubGlobal('navigator', { ...navigator, clipboard:{writeText} })
  try {
    render(<><TableIdentifier value={value}/><TableText value={'备注'.repeat(60)} label="备注"/></>)
    const summary = screen.getByLabelText('查看完整标识')
    fireEvent.click(summary)
    expect(summary.closest('details')!.open).toBe(true)
    expect(screen.getByText(value)).toBeTruthy()
    expect(screen.getByLabelText('查看完整备注').closest('details')!.textContent).toContain('备注'.repeat(60))
    fireEvent.click(screen.getByRole('button',{name:/Copy|复制/i}))
    await waitFor(()=>expect(writeText).toHaveBeenCalledWith(value))
  } finally {
    vi.unstubAllGlobals()
  }
})
