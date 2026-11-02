import {afterEach,expect,it,vi} from 'vitest'
import {executeResourceAction} from '../src/features/resources/actions'
afterEach(()=>vi.unstubAllGlobals())
it('uses actual supported page APIs and copies external links instead of pretending to open them',async()=>{
 const uni={switchTab:vi.fn(),navigateTo:vi.fn(),setClipboardData:vi.fn(),makePhoneCall:vi.fn()};vi.stubGlobal('uni',uni)
 await executeResourceAction({kind:'internal_page',page:'feed'})
 await executeResourceAction({kind:'internal_page',page:'post',object_id:'p&other'})
 await executeResourceAction({kind:'external_link',url:'https://school.example/help'})
 await executeResourceAction({kind:'phone',number:'10086'})
 expect(uni.switchTab).toHaveBeenCalledWith({url:'/pages/index/index'})
 expect(uni.navigateTo).toHaveBeenCalledOnce()
 expect(uni.navigateTo).toHaveBeenCalledWith({url:'/pages/index/detail?id=p%26other'})
 expect(uni.setClipboardData).toHaveBeenCalledWith({data:'https://school.example/help'})
 expect(uni.makePhoneCall).toHaveBeenCalledWith({phoneNumber:'10086'})
})
it('propagates failed operations so the caller never records a successful click',async()=>{
 vi.stubGlobal('uni',{setClipboardData:vi.fn().mockRejectedValue(new Error('拒绝复制'))})
 await expect(executeResourceAction({kind:'copy_text',text:'服务电话'})).rejects.toThrow('拒绝复制')
})
