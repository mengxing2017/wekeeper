import {expect,it} from 'vitest'
import {createHash} from 'node:crypto'
import f from '../fixtures/shared-mobile-send-v9.json'
import {createSharedSendV2Client,SHARED_SEND_SCHEMA} from '../src/modules/mobile-execution/shared-send/v2'
import {createClient} from '../src/http/client'
function canonicalJson(v:unknown):string {if(v===null||typeof v!=='object')return JSON.stringify(v);if(Array.isArray(v))return '['+v.map(canonicalJson).join(',')+']';const r=v as Record<string,unknown>;return '{'+Object.keys(r).sort().map(k=>JSON.stringify(k)+':'+canonicalJson(r[k])).join(',')+'}'}
it('v9 preserves the full generic wire, independent digest and JSON:API management resource',async()=>{
 const hash=(v:unknown)=>createHash('sha256').update(canonicalJson(v)).digest('hex')
 expect(hash(f.flow)).toBe(f.flow_digest);expect(hash(f.event)).toBe(f.receipt.body_hash)
 expect(f.event.schema).toBe(SHARED_SEND_SCHEMA)
 const calls:unknown[]=[]
 const client=createClient(async r=>{calls.push(JSON.parse(r.body as string));return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'mobile-v9-management',id:'new',attributes:{}}}}})
 await createSharedSendV2Client(client).manage({action:'activations',input:{release_id:'release',expected_revision:2,reason:'验收完成'}},'stable-key')
 expect(createSharedSendV2Client(client).artifactUrl('file-id')).toBe('/api/v1/platform/mobile-execution/shared-v2-artifacts/file-id/content')
 expect(calls).toEqual([{data:{type:'mobile-v9-management',attributes:{action:'activations',input:{release_id:'release',expected_revision:2,reason:'验收完成'}}}}])
})
