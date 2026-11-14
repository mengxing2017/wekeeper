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
 const payload={algorithm:fixture.algorithm,event_type:event.event_type,version:event.version,title:event.title,content:event.content,detail_url:event.detail_url}
 expect(JSON.stringify(payload)).toBe(fixture.canonical)
 expect(createHash('sha256').update(JSON.stringify(payload)).digest('hex')).toBe(fixture.sha256)
 for(const [path,hash] of Object.entries(manifest.files_sha256))expect(createHash('sha256').update(readFileSync(new URL('../'+path,import.meta.url))).digest('hex'),path).toBe(hash)
 // This field tells callers about request resubmission, independently of sending completion.
 const wire=receipt.data.attributes as SourceReceiptV2
 expect(wire.retryable).toBe(false);expect(wire.sending).toBeNull()
})
it('uses receipt queries without content control commands',async()=>{
 const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false});const client=createSourceReceiptsClient({requestResource});await client.list('route',2);expect(requestResource).toHaveBeenCalledWith('/api/v2/admin/source-routes/route/receipts?page=2');expect('recheck' in client).toBe(false)
})
