import previewItem from '../fixtures/shared-mobile-preview-item.json'
import type {SharedPreview,SharedReadiness} from '../src/modules/mobile-execution/shared-send/client'
import { createHash } from 'node:crypto'
import delegation from '../fixtures/shared-mobile-delegation.json'
import { describe, it, expect } from 'vitest'
import { resourceType } from '../src/http/protocol'
import { createSharedSendClient } from '../src/modules/mobile-execution/shared-send/client'
import fixture from '../fixtures/shared-mobile-send-v8.json'

describe('shared v8 contracts', () => {
 it('uses action resource types and keeps schools in the path', () => {
  const base='/api/v1/schools/school/mobile-execution'
  for(const [path,type] of [['shared-task-drafts','drafts'],['shared-task-previews','previews'],['shared-tasks','tasks'],['shared-tasks/id/stops','stops'],['shared-items/id/reviews','reviews'],['shared-items/id/reassignments','reassignments'],['shared-inspections','inspections']])expect(resourceType(`${base}/${path}`)).toBe(`mobile-shared-${type}`)
 })
 it('keeps literal text and fixed caller keys', async () => {
  const calls:unknown[][]=[]
  const client=createSharedSendClient({requestResource:async (...args:unknown[])=>{calls.push(args);return {} as never}})
  await client.draft('a/b',{target_ids:['target'],message:fixture.message,content_type:'text/plain'},'stable-key')
  expect(calls[0][0]).toBe('/api/v1/schools/a%2Fb/mobile-execution/shared-task-drafts')
  expect(calls[0][2]).toEqual({target_ids:['target'],message:fixture.message,content_type:'text/plain'})
  expect(calls[0][4]).toEqual({'Idempotency-Key':'stable-key'})
 })
})

it('shares a canonical stable delegation without release or target dependencies', () => {
 const canonical=JSON.stringify(Object.fromEntries(Object.entries(delegation.scope).sort(([a],[b])=>a.localeCompare(b))))
 expect(createHash('sha256').update(canonical).digest('hex')).toBe(delegation.scope_digest)
 expect(delegation.decision.scope_digest).toBe(delegation.scope_digest)
 expect(delegation.scope).not.toHaveProperty('release_id')
 expect(delegation.scope).not.toHaveProperty('target_id')
})
it('scopes school self-service binding and revision-guards pool access', async () => {
 const calls:unknown[][]=[]
 const client=createSharedSendClient({requestResource:async (...args:unknown[])=>{calls.push(args);return {} as never}})
 await client.bind('a/b',{target_id:'t',device_id:'d',expected_access_revision:3,search_text_override:null},'bind-key')
 await client.access({school_id:'a/b',device_id:'d',expected_revision:3,enabled:false},'revoke-key')
 expect(calls[0][0]).toBe('/api/v1/schools/a%2Fb/mobile-execution/shared-bindings')
 expect(calls[0][2]).toMatchObject({expected_access_revision:3})
 expect(calls[1][2]).toMatchObject({expected_revision:3,enabled:false})
 expect(resourceType(String(calls[0][0]))).toBe('mobile-shared-bindings')
 expect(resourceType(String(calls[1][0]))).toBe('mobile-shared-access')
})

it('keeps target and bound phone names independent from selectable phones',()=>{
 const item:SharedPreview['plan']['items'][number]={...previewItem,readiness:previewItem.readiness as SharedReadiness[]}
 expect(item.selected).toBeNull()
 expect(item.candidates).toHaveLength(0)
 expect(item.target.name).toBe('家长通知群')
 expect(item.readiness![0].device_name).toBe('共享手机甲')
 expect(item.readiness![0].inspection_state).toBe('not_scheduled')
})
