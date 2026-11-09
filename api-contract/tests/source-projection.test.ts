import {afterEach,expect,it,vi} from 'vitest'
import {createHash} from 'node:crypto'
import {readFileSync} from 'node:fs'
import {sourceReceipts,sourceRouting,type SourceReceiptV2} from '../src/modules/source-access'
import {createClient} from '../src/http/client'
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
it('uses scoped client paths and dedicated recheck type with the expected revision',async()=>{
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json({data:{type:'source-receipt-rechecks',id:'result',attributes:{queued:true}}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 await sourceReceipts.list('route-one',2)
 await sourceReceipts.detail('route-one','receipt-one')
 await sourceReceipts.recheck('route-one','receipt-one',9,'规则复核')
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/source-routes/route-one/receipts?page=2')
 expect(fetchMock.mock.calls[1][0]).toBe('/api/v2/admin/source-routes/route-one/receipts/receipt-one')
 expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({data:{type:'source-receipt-rechecks',attributes:{revision:9,reason:'规则复核'}}})
 await sourceRouting.manager('route',3,'operator',true,true)
 expect(JSON.parse(fetchMock.mock.calls[3][1].body).data.attributes.can_recheck).toBe(true)
})
it('supports a platform-neutral transport with session CSRF on recheck writes',async()=>{
 const transport=vi.fn().mockResolvedValue({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'source-receipt-rechecks',id:'r',attributes:{queued:true}}}})
 const client=createClient(transport);client.setCsrf('csrf-test')
 await createSourceReceiptsClient(client).recheck('route','receipt',2,'复核')
 expect(transport.mock.calls[0][0].headers['X-CSRF-Token']).toBe('csrf-test')
})
