import {it,expect} from 'vitest'
import {createHash,createPublicKey,verify} from 'node:crypto'
import fixture from '../fixtures/durable-execution.json'
import {durableExecutionApi,type ExecutionCompletion} from '../src/modules/mobile-execution/shared-send/v2'
import {createClient} from '../src/http/client'
const canonical=(v:unknown):string=>Array.isArray(v)?`[${v.map(canonical).join(',')}]`:v!==null&&typeof v==='object'?`{${Object.entries(v).sort(([a],[b])=>a.localeCompare(b,'en')).map(([k,x])=>`${JSON.stringify(k)}:${canonical(x)}`).join(',')}}`:JSON.stringify(v)
it('shares signed grants and immutable completion payloads with Rust and Kotlin',()=>{
 const {signature,key_id,...body}=fixture.grant
 const point=Buffer.from(fixture.verification_key,'hex')
 const key=createPublicKey({key:Buffer.concat([Buffer.from('3059301306072a8648ce3d020106082a8648ce3d030107034200','hex'),point]),format:'der',type:'spki'})
 expect(verify('sha256',Buffer.from(canonical(body)),key,Buffer.from(signature,'hex'))).toBe(true)
 expect(createHash('sha256').update(fixture.verification_key).digest('hex')).toBe(key_id)
})
it('uses only the new completion resource and stable result identity',async()=>{
 const calls:Array<{path:string;body:unknown;key:string|undefined}>=[]
 const client=createClient(async r=>{calls.push({path:r.path,body:JSON.parse(r.body as string),key:r.headers['Idempotency-Key']});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{id:fixture.completion.result_id,type:'execution-completions',attributes:{recorded:true}}}}})
 const body:ExecutionCompletion={...fixture.completion,outcome:'observed_sent'}
 await durableExecutionApi(client).complete(body)
 expect(calls[0]).toEqual({path:'/api/v10/device-execution/execution-completions',body:{data:{type:'execution-completions',attributes:body}},key:body.result_id})
})
