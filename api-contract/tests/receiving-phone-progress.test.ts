import {expect,it,vi} from 'vitest'
import {createMessageClient,type PhoneProgress} from '../src/modules/source-access/automation-client'
import fixture from '../fixtures/receiving-phone-progress.json'
it('uses project management endpoints without adding fields to external source or phone protocols',async()=>{
 const requestResource=vi.fn().mockResolvedValue(fixture)
 const client=createMessageClient({requestResource})
 const view:PhoneProgress=await client.phoneDeliveries(fixture.id,2)
 expect(view.summary.expected_count).toBe(3)
 expect(requestResource).toHaveBeenLastCalledWith(`/api/v2/admin/messages/${fixture.id}/phone-deliveries?page[number]=2&page[size]=30`)
 await client.stopPhones(fixture.id,fixture.targets[0].target_id)
 expect(requestResource).toHaveBeenLastCalledWith(`/api/v2/admin/messages/${fixture.id}/phone-targets/${fixture.targets[0].target_id}/stop`,'POST')
})
