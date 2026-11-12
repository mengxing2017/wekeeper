import {afterEach,expect,it,vi} from 'vitest'
import {businessDirectory} from '../src/modules/business-directory'
import fixture from '../fixtures/business-directory.json'

afterEach(()=>vi.unstubAllGlobals())

it('reads platform and unit directory pages without route or workspace identifiers',async()=>{
 const fetchMock=vi.fn()
  .mockResolvedValueOnce(Response.json({data:{type:'business-platforms',id:'list',attributes:fixture.platforms_page}},{headers:{'content-type':'application/vnd.api+json'}}))
  .mockResolvedValueOnce(Response.json({data:{type:'business-units',id:'list',attributes:fixture.units_page}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await businessDirectory.platforms()).toMatchObject(fixture.platforms_page)
 expect(await businessDirectory.units('11111111-1111-4111-8111-111111111111',2)).toMatchObject(fixture.units_page)
 expect(fetchMock.mock.calls[0][0]).toBe('/api/v2/admin/business-platforms?page=1')
 expect(fetchMock.mock.calls[1][0]).toBe('/api/v2/admin/business-platforms/11111111-1111-4111-8111-111111111111/units?page=2')
})
