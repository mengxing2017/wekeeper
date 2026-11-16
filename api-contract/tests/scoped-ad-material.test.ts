import {afterEach,expect,it,vi} from 'vitest'
import {adPlacements,scopedMaterials} from '../src/modules/advertising'
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

it('creates a direct unit-scoped advertisement placement without route or workspace fields',async()=>{
 const fetchMock=vi.fn().mockImplementationOnce(async()=>Response.json(fixture.placement_response,{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json(fixture.placement_response,{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json(fixture.placement_state_response,{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await adPlacements.create(fixture.placement_create as Parameters<typeof adPlacements.create>[0])).toEqual(fixture.placement_view)
 expect(await adPlacements.get(fixture.placement_view.id)).toEqual(fixture.placement_view)
 expect(await adPlacements.setState(fixture.placement_view.id,fixture.placement_state)).toEqual(fixture.placement_state_view)
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/ad-placements')
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({data:{type:'ad-placements',attributes:fixture.placement_create}})
 expect(fetchMock.mock.calls[1][0]).toBe(`/api/v2/admin/ad-placements/${fixture.placement_view.id}`)
 expect(fetchMock.mock.calls[2][0]).toBe(`/api/v2/admin/ad-placements/${fixture.placement_view.id}/state`)
 expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({data:{type:'ad-placement-states',attributes:fixture.placement_state}})
})

it('lists independent material and placement resources',async()=>{
 const fetchMock=vi.fn().mockImplementationOnce(async()=>Response.json({data:{type:'ad-materials',id:'list',attributes:{items:[fixture.material],has_more:false}}},{headers:{'content-type':'application/vnd.api+json'}})).mockImplementationOnce(async()=>Response.json({data:{type:'ad-placements',id:'list',attributes:{items:[fixture.placement_view],has_more:false}}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await scopedMaterials.list()).toMatchObject({items:[fixture.material],has_more:false})
 expect(await adPlacements.list(2)).toMatchObject({items:[fixture.placement_view],has_more:false})
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/ad-materials?page%5Bnumber%5D=1&page%5Bsize%5D=30')
 expect(fetchMock.mock.calls[1][0]).toBe('/api/v2/admin/ad-placements?page%5Bnumber%5D=2&page%5Bsize%5D=30')
})

it('archives an independent material with its revision without an operation reason',async()=>{
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
 expect(fetchMock.mock.calls[0][0]).toBe(`/api/v2/admin/ad-materials/${fixture.material.id}/versions?page%5Bnumber%5D=2&page%5Bsize%5D=30`)
})
