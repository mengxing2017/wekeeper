import {expect,it,vi} from 'vitest'
import {createMessageClient} from '../src/modules/source-access/automation-client'
it('queries message results with no workspace or mutation controls',async()=>{const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false});const client=createMessageClient({requestResource});await client.list('route','receipt',2);expect(requestResource).toHaveBeenCalledWith('/api/v2/admin/messages?route_id=route&receipt_id=receipt&page%5Bnumber%5D=2&page%5Bsize%5D=30');expect('stop' in client).toBe(false);expect('retry' in client).toBe(false)})

import fixture from '../fixtures/business-message.json'
import type {BusinessMessage} from '../src/modules/source-access/automation-client'
it('uses the Rust serialized message result fixture',()=>{const message:BusinessMessage=fixture;expect(message.reason_code).toBe('route_no_targets');expect(message.delegates).toHaveLength(0)})
