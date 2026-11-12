import {afterEach,expect,it,vi} from 'vitest'
import {resourceType} from '../src/http/protocol'
import {sourceRouting,type RouteConfigurationInput,type RoutingSnapshot,type RoutingCheck} from '../src/modules/source-access'
import fixture from '../fixtures/source-routing.json'
afterEach(()=>vi.unstubAllGlobals())
it('sends an atomic typed configuration and decodes the Rust snapshot fixture',async()=>{
 const snapshot=fixture.snapshot as RoutingSnapshot
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json({data:{type:'source-route-configurations',id:'fixture-route',attributes:{revision:7,snapshot}}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 const v=await sourceRouting.configuration('fixture-route');expect(v.snapshot).toEqual(snapshot)
 await sourceRouting.configure('fixture-route',fixture.input as RouteConfigurationInput)
 expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({data:{type:'source-route-configurations',attributes:fixture.input}})
})
it('keeps lifecycle, organization and credential grant types distinct',()=>{
 const routes:Record<string,string>={
  '/api/v2/admin/routing/routing-units':'routing-units',
  '/api/v2/admin/routing/routing-groups/group':'routing-groups',
  '/api/v2/admin/source-routes/route/lifecycle':'source-route-lifecycle',
  '/api/v2/admin/source-routes/route/managers':'source-route-managers',
  '/api/v2/admin/source-routes/route/handoff':'source-route-handoffs',
  '/api/v2/admin/source-credentials/key/routes/route':'source-key-routes',
 }
 for(const [path,type] of Object.entries(routes))expect(resourceType(path)).toBe(type)
})
it('preserves explicit resource errors without invoking a second channel',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(Response.json({data:{type:'source-route-checks',id:'result',attributes:fixture.check}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 const check:RoutingCheck=await sourceRouting.check('fixture-route');expect(check.issues).toEqual(['mobile_candidates_missing']);expect(fetchMock).toHaveBeenCalledTimes(1)
})
