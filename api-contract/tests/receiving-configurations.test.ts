import {afterEach, expect, it, vi} from 'vitest'
import {mobileReceivingConfigurations, wecomReceivingConfigurations} from '../src/modules/receiving-configurations'
import fixture from '../fixtures/receiving-configuration.json'

afterEach(() => vi.unstubAllGlobals())

it.each([
  ['mobile-execution', 'mobile-receiving-configurations', mobileReceivingConfigurations],
  ['wecom-distribution', 'wecom-receiving-configurations', wecomReceivingConfigurations],
] as const)('uses channel-owned %s resources and shared serialization', async (channel, type, client) => {
  const configuration = channel === 'mobile-execution' ? fixture.configuration : fixture.wecom_configuration
  const {id, ...attributes} = configuration
  const response = {data: {type, id, attributes}}
  const fetchMock = vi.fn().mockImplementation(async () => Response.json(response, {headers: {'content-type': 'application/vnd.api+json'}}))
  vi.stubGlobal('fetch', fetchMock)
  const input = channel === 'mobile-execution' ? fixture.input : fixture.wecom_input
  expect(await client.save(configuration.business_unit_id, id, {...input,client_kind:channel==='mobile-execution'?'wecom':undefined,target_selection_mode:'all',device_dispatch_mode:channel==='mobile-execution'?'round_robin':null})).toEqual(configuration)
  const base = `/api/v1/${channel}/business-units/${configuration.business_unit_id}/receiving-configurations`
  expect(fetchMock.mock.calls[0][0]).toBe(`${base}/${id}`)
  expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data: {type, attributes: input}})
  await client.get(configuration.business_unit_id, id)
  expect(fetchMock.mock.calls[1][0]).toBe(`${base}/${id}`)
  fetchMock.mockImplementationOnce(async () => Response.json({data: [response.data], meta: {page: {number: 1, size: 30, has_more: false}}}, {headers: {'content-type': 'application/vnd.api+json'}}))
  expect(await client.list(configuration.business_unit_id)).toMatchObject({items: [configuration], page: {has_more: false}})
  expect(fetchMock.mock.calls[2][0]).toBe(`${base}?page[number]=1&page[size]=30`)
})

it('creates a receiving target directly in a business unit',async()=>{
 const input={...fixture.target_input,client_kind:"wecom" as const};const unit=fixture.configuration.business_unit_id,id=fixture.target.id
 const fetchMock=vi.fn().mockResolvedValue(Response.json({data:{type:'receiving-targets',id,attributes:{name:input.name}}},{headers:{'content-type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetchMock)
 expect(await mobileReceivingConfigurations.target(unit,id,input)).toEqual({id,name:'接收群'})
 expect(fetchMock.mock.calls[0][0]).toBe(`/api/v1/mobile-execution/business-units/${unit}/receiving-targets/${id}`)
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data:{type:'receiving-targets',attributes:input}})
})
