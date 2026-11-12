import {afterEach,expect,it,vi} from 'vitest'
import {placementRules,scopedMaterials} from '../src/modules/advertising'
import fixture from '../fixtures/scoped-ad-material.json'

afterEach(()=>vi.unstubAllGlobals())

it('uses the independent material resources and shared payloads',async()=>{
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json(fixture.response,{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await scopedMaterials.create(fixture.create)).toEqual(fixture.material)
 expect(await scopedMaterials.get(fixture.material.id)).toEqual(fixture.material)
 expect(await scopedMaterials.publish(fixture.material.id,fixture.publish)).toEqual(fixture.material)
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/ad-materials')
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data:{type:'ad-materials',attributes:fixture.create}})
 expect(fetchMock.mock.calls[1][0]).toBe(`/api/v2/admin/ad-materials/${fixture.material.id}`)
 expect(fetchMock.mock.calls[2][0]).toBe(`/api/v2/admin/ad-materials/${fixture.material.id}/publications`)
 expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({data:{type:'ad-material-publications',attributes:fixture.publish}})
})

it('creates a unit-scoped rule without route or workspace fields',async()=>{
 const fetchMock=vi.fn().mockImplementationOnce(async()=>Response.json(fixture.rule_response,{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json(fixture.rule_response,{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json(fixture.rule_state_response,{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await placementRules.create(fixture.rule_create as Parameters<typeof placementRules.create>[0])).toEqual(fixture.rule_view)
 expect(await placementRules.get(fixture.rule_view.id)).toEqual(fixture.rule_view)
 expect(await placementRules.setState(fixture.rule_view.id,fixture.rule_state)).toEqual(fixture.rule_state_view)
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/ad-placement-rules')
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data:{type:'ad-placement-rules',attributes:fixture.rule_create}})
 expect(fetchMock.mock.calls[1][0]).toBe(`/api/v2/admin/ad-placement-rules/${fixture.rule_view.id}`)
 expect(fetchMock.mock.calls[2][0]).toBe(`/api/v2/admin/ad-placement-rules/${fixture.rule_view.id}/state`)
 expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({data:{type:'ad-placement-rule-states',attributes:fixture.rule_state}})
})

it('lists independent material and placement resources',async()=>{
 const fetchMock=vi.fn().mockImplementationOnce(async()=>Response.json({data:{type:'ad-materials',id:'list',attributes:{items:[fixture.material],has_more:false}}},{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json({data:{type:'ad-placement-rules',id:'list',attributes:{items:[fixture.rule_view],has_more:false}}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await scopedMaterials.list()).toMatchObject({items:[fixture.material],has_more:false})
 expect(await placementRules.list(2)).toMatchObject({items:[fixture.rule_view],has_more:false})
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/ad-materials?page=1')
 expect(fetchMock.mock.calls[1][0]).toBe('/api/v2/admin/ad-placement-rules?page=2')
})

it('archives an independent material with its revision and reason',async()=>{
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json(fixture.response,{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 await scopedMaterials.archive(fixture.material.id,fixture.archive)
 expect(fetchMock.mock.calls[0][0]).toBe(`/api/v2/admin/ad-materials/${fixture.material.id}/archive`)
 expect(fetchMock.mock.calls[0][1].method).toBe('PUT')
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data:{type:'ad-material-archives',attributes:fixture.archive}})
})

it('reads immutable material versions',async()=>{
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json({data:{type:'ad-material-versions',id:'list',attributes:{items:[fixture.material_version],has_more:false}}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await scopedMaterials.versions(fixture.material.id,2)).toMatchObject({items:[fixture.material_version],has_more:false})
 expect(fetchMock.mock.calls[0][0]).toBe(`/api/v2/admin/ad-materials/${fixture.material.id}/versions?page=2`)
})
