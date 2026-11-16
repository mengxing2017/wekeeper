import {afterEach,expect,it,vi} from 'vitest'
import {createHash} from 'node:crypto'
import {readFileSync} from 'node:fs'
import {type SourceReceiptV2} from '../src/modules/source-access'
import {createSourceReceiptsClient} from '../src/modules/source-access/receipts-client'
import fixture from '../fixtures/source-projection.json'
import receipt from '../fixtures/source-receipt-v2.json'
import manifest from '../source-v2/manifest.json'
afterEach(()=>vi.unstubAllGlobals())
it('locks semantic payload bytes independently of signed request bytes',()=>{
 const {event}=fixture
 const payload={algorithm:fixture.algorithm,event_type:event.event_type,content:event.content}
 expect(JSON.stringify(payload)).toBe(fixture.canonical)
 expect(createHash('sha256').update(JSON.stringify(payload)).digest('hex')).toBe(fixture.sha256)
 for(const [path,hash] of Object.entries(manifest.files_sha256))expect(createHash('sha256').update(readFileSync(new URL('../'+path,import.meta.url))).digest('hex'),path).toBe(hash)
 // Machine callers only see durable acceptance.
 const wire=receipt.data.attributes as SourceReceiptV2
 expect(wire.status).toBe('received');expect(Object.keys(wire).sort()).toEqual(['event_id','received_at','status'])
})
it('uses receipt queries without content control commands',async()=>{
 const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false});const client=createSourceReceiptsClient({requestResource});await client.list('route',2);expect(requestResource).toHaveBeenCalledWith('/api/v2/admin/source-routes/route/receipts?page%5Bnumber%5D=2&page%5Bsize%5D=30');expect('recheck' in client).toBe(false)
})
