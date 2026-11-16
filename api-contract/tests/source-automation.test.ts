import {expect,it,vi} from 'vitest'
import {createMessageClient} from '../src/modules/source-access/automation-client'
it('queries message results with no workspace or mutation controls',async()=>{const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false});const client=createMessageClient({requestResource});await client.list('route','receipt',2);expect(requestResource).toHaveBeenCalledWith('/api/v2/admin/messages?route_id=route&receipt_id=receipt&page%5Bnumber%5D=2&page%5Bsize%5D=30');expect('stop' in client).toBe(false);expect('retry' in client).toBe(false)})

import fixture from '../fixtures/business-message.json'
import type {BusinessMessage} from '../src/modules/source-access/automation-client'
it('uses the Rust serialized message result fixture',()=>{const message:BusinessMessage=fixture;expect(message.reason_code).toBe('route_no_targets');expect(message.delegates).toHaveLength(0)})

it('encodes progress filters and paginated receipt target queries',async()=>{
 const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false});const client=createMessageClient({requestResource})
 await client.list('', '', 2, 1, {channel:'mobile',stage:'device_receipt',attention:'attention'})
 expect(requestResource).toHaveBeenLastCalledWith('/api/v2/admin/messages?channel=mobile&stage=device_receipt&attention=attention&page%5Bnumber%5D=2&page%5Bsize%5D=1')
 await client.deliveries('receipt/id',2,1)
 expect(requestResource).toHaveBeenLastCalledWith('/api/v2/admin/messages/receipt%2Fid/deliveries?page%5Bnumber%5D=2&page%5Bsize%5D=1')
})

import delivery from '../fixtures/business-message-delivery.json'
import type {MessageDelivery} from '../src/modules/source-access/automation-client'
it('shares the Rust serialized target diagnostic',()=>{const item:MessageDelivery=delivery;expect(item.diagnostic.stage).toBe('device_receipt');expect(item.outcome).toBe('pending')})
