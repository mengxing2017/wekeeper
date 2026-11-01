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
