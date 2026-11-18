import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,render,screen} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {PhoneDeliveries} from './phone-deliveries'
const mock=vi.hoisted(()=>({phoneDeliveries:vi.fn(),stopPhones:vi.fn()}))
vi.mock('./api',()=>({businessMessages:mock}))
afterEach(cleanup)
it('shows independent phone outcomes and keeps parent counts distinct',async()=>{
 mock.phoneDeliveries.mockResolvedValue({summary:{target_count:1,expected_count:3,materialized_count:3,sent:1,failed:1,unknown:1,pending:0},targets:[],items:[
 {id:'a',target_name:'接收群',device_name:'手机 A',outcome:'sent',stage:'sent',reason_code:'observed_sent'},
 {id:'b',target_name:'接收群',device_name:'手机 B',outcome:'failed',stage:'failed',reason_code:'device_unavailable'},
 {id:'c',target_name:'接收群',device_name:'手机 C',outcome:'unknown',stage:'unknown',reason_code:'unknown'},
 ],has_more:false})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><PhoneDeliveries receipt="receipt"/></QueryClientProvider>)
 expect(await screen.findByText(/1 个群，3 个手机投递项/)).toBeTruthy()
 expect(screen.getByText('部分成功，其他手机的责任仍独立保留')).toBeTruthy()
 expect(screen.getByText('手机反馈发送成功')).toBeTruthy()
 expect(screen.getByText('未完成发送')).toBeTruthy()
 expect(screen.getByText('结果未知')).toBeTruthy()
 expect(mock.phoneDeliveries).toHaveBeenCalledWith('receipt',1)
})
