import {afterEach,expect,it,vi} from 'vitest'
import {sourceRouting,type RouteConfiguration} from '../src/modules/source-access'
import fixture from '../fixtures/source-route-configuration.json'
afterEach(()=>vi.unstubAllGlobals())
it('reads administrator route configuration using the shared Rust response fixture',async()=>{
 const fetchMock=vi.fn<typeof fetch>(async()=>Response.json(fixture,{headers:{'content-type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetchMock)
 const configuration:RouteConfiguration=await sourceRouting.configuration('fixture-route')
 expect(configuration).toEqual({...fixture.data.attributes,id:fixture.data.id})
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/source-routes/fixture-route/configuration')
 expect('manager' in sourceRouting).toBe(false)
 expect('configure' in sourceRouting).toBe(false);expect('workspaces' in sourceRouting).toBe(false)
})
