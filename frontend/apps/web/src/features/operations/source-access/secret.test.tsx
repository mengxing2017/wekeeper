import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {afterEach,expect,it,vi} from 'vitest'
import {CallerSecret} from './secret'
const api=vi.hoisted(()=>({revealSecret:vi.fn()}))
vi.mock('./api',()=>({sourceAccess:api}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('reveals the same key repeatedly, hides it and allows recovery after a failed read',async()=>{
 api.revealSecret.mockResolvedValue({id:'key-1',key_id:'key-1',secret:'fixture-signing-secret'})
 render(<CallerSecret keyId="key-1"/>)
 expect(api.revealSecret).not.toHaveBeenCalled()
 fireEvent.click(screen.getByRole('button',{name:'查看密钥'}))
 expect((await screen.findByRole('textbox',{name:'签名密钥（secret）'}) as HTMLTextAreaElement).value).toBe('fixture-signing-secret')
 expect(screen.getByText('复制密钥')).toBeTruthy()
 expect(screen.getByRole('button',{name:'下载凭据'})).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:'隐藏密钥'}))
 expect(screen.queryByRole('textbox')).toBeNull()
 api.revealSecret.mockRejectedValueOnce(new Error('暂时不可用'))
 fireEvent.click(screen.getByRole('button',{name:'查看密钥'}))
 expect(await screen.findByText('暂时不可用')).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:'查看密钥'}))
 await waitFor(()=>expect((screen.getByRole('textbox',{name:'签名密钥（secret）'}) as HTMLTextAreaElement).value).toBe('fixture-signing-secret'))
 expect(api.revealSecret).toHaveBeenCalledTimes(3)
})
