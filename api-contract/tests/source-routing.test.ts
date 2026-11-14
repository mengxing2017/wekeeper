import {afterEach,expect,it,vi} from 'vitest'
import {sourceRouting} from '../src/modules/source-access'
afterEach(()=>vi.unstubAllGlobals())
it('manages intake access without sending configuration or version controls',async()=>{
 const fetchMock=vi.fn(async()=>Response.json({data:{type:'source-route-managers',id:'route',attributes:{}}},{headers:{'content-type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetchMock)
 await sourceRouting.manager('route','operator',true)
 expect(fetchMock).toHaveBeenCalled()
 expect('configure' in sourceRouting).toBe(false);expect('workspaces' in sourceRouting).toBe(false)
})
