import { afterEach, expect, it, vi } from 'vitest'
import { auditEvents } from '../src/modules/business'
afterEach(() => vi.unstubAllGlobals())
it('reads system audit independently of distribution workspaces with pagination', async () => {
  const fetch = vi.fn().mockResolvedValue(Response.json({data:[{type:'audit-events',id:'event',attributes:{actor_id:'admin',action:'identity.password.reset',object_id:'account',request_id:'request',detail:{},created_at:'2026-11-08T00:00:00Z'}}],meta:{page:{number:2,size:30,has_more:false}}},{headers:{'Content-Type':'application/vnd.api+json'}}))
  vi.stubGlobal('fetch',fetch)
  const result = await auditEvents.list('?page[number]=2&search=password')
  expect(fetch).toHaveBeenCalledWith('/api/v1/audit-events?page[number]=2&search=password',expect.objectContaining({credentials:'same-origin'}))
  expect(result.items[0].action).toBe('identity.password.reset')
  expect(result.page).toEqual({number:2,size:30,has_more:false})
})
