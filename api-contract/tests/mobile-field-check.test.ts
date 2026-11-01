import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi } from '../src/modules/mobile-execution'
import { createMobileExecutionDeviceClient } from '../src/modules/mobile-execution/device'
import type { FieldCheckResultInput } from '../src/modules/mobile-execution/capabilities'
import fixture from '../fixtures/shared-mobile-field-check-result.json'
afterEach(() => vi.unstubAllGlobals())
test('school invites exact binding revisions; platform requests only an invitation', async () => {
 const transport=vi.fn<typeof fetch>().mockImplementation(async (url) => new Response(JSON.stringify({data:{type:String(url).includes('invitations')?'mobile-field-check-invitations':'mobile-field-checks',id:'id',attributes:{}}}),{headers:{'Content-Type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',transport)
 const invitation={binding_id:'binding',expected_binding_revision:2,expected_target_locator_revision:3}
 await mobileExecutionApi.inviteFieldCheck('school',invitation,'invite-key')
 await mobileExecutionApi.requestFieldCheck({opening_id:'opening',invitation_id:'invite',expected_invitation_revision:1},'request-key')
 expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/field-check-invitations')
 expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({data:{type:'mobile-field-check-invitations',attributes:invitation}})
 expect(transport.mock.calls[1][0]).toBe('/api/v1/platform/mobile-execution/field-checks')
 expect(JSON.parse(String(transport.mock.calls[1][1]?.body)).data.attributes).not.toHaveProperty('search_text')
})
test('fixed field evidence retains its fence, challenge and idempotency key over retry', async () => {
 const transport=vi.fn<typeof fetch>().mockImplementation(async () => new Response(JSON.stringify({data:{type:fixture.data.type,id:fixture.data.id,attributes:{disposition:'passed',body:fixture.data.attributes}}}),{headers:{'Content-Type':'application/vnd.api+json'}}))
 const client=createMobileExecutionDeviceClient({baseUrl:'https://keeper.example',token:'wk_device_test',fetch:transport})
 const body=fixture.data.attributes as FieldCheckResultInput
 await client.fieldCheckResult(fixture.data.id,body,'fixed-result')
 await client.fieldCheckResult(fixture.data.id,body,'fixed-result')
 expect(transport.mock.calls[0]).toEqual(transport.mock.calls[1])
 expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({data:{type:fixture.data.type,attributes:fixture.data.attributes}})
 expect(transport.mock.calls[0][1]?.headers).toMatchObject({Authorization:'Bearer wk_device_test','Idempotency-Key':'fixed-result'})
 await expect(client.claimFieldCheck('check',1,'')).rejects.toThrow('stable Idempotency-Key')
 expect(transport).toHaveBeenCalledTimes(2)
})
