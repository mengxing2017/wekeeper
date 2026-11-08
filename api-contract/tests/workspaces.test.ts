import { afterEach, expect, it, vi } from 'vitest'
import { wecomWorkspaces, officialSenders } from '../src/modules/wecom-distribution'
import { mobileWorkspaces } from '../src/modules/mobile-execution'
import { identity } from '../src/modules/business'
afterEach(() => vi.unstubAllGlobals())
it('uses independent scope/member resources for both distribution channels', async () => {
 const fetch = vi.fn().mockImplementation(() => Promise.resolve(Response.json({data:null},{headers:{'Content-Type':'application/vnd.api+json'}})))
 vi.stubGlobal('fetch',fetch)
 await wecomWorkspaces.save('a','通知')
 await wecomWorkspaces.grant('a','operator','admin')
 await mobileWorkspaces.grant('b','operator','revoked')
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/wecom-distribution/workspaces/a')
 expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'wecom-workspaces',attributes:{name:'通知'}}})
 expect(fetch.mock.calls[1][0]).toBe('/api/v1/wecom-distribution/workspaces/a/members')
 expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({data:{type:'wecom-workspace-members',attributes:{username:'operator',role:'admin'}}})
 expect(fetch.mock.calls[2][0]).toBe('/api/v1/mobile-execution/workspaces/b/members')
 expect(JSON.parse(fetch.mock.calls[2][1].body).data.type).toBe('mobile-workspace-members')
})
it('retains sender qualification and dedicated administrator reset without account CRUD', async () => {
 const fetch = vi.fn().mockImplementation(() => Promise.resolve(Response.json({data:null},{headers:{'Content-Type':'application/vnd.api+json'}})))
 vi.stubGlobal('fetch',fetch)
 await officialSenders.qualify('a','u',{enabled:false,revision:1})
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/platform/wecom-distribution/workspaces/a/wecom-senders/u/qualification')
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.type).toBe('wecom-senders')
 await identity.resetAdministrator('u','new-password-123')
 expect(fetch.mock.calls[1][0]).toBe('/api/v1/platform/administrators/u/password')
 expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({data:{type:'administrators',attributes:{password:'new-password-123'}}})
 for (const method of ['accounts','create','update','reset']) expect(identity).not.toHaveProperty(method)
})
it('queries and maintains only the dedicated system administrator resources', async () => {
 const fetch = vi.fn().mockImplementation(() => Promise.resolve(Response.json({data:null},{headers:{'Content-Type':'application/vnd.api+json'}})))
 vi.stubGlobal('fetch', fetch)
 await identity.administrators('王 & admin')
 await identity.createAdministrator({username:'new-admin',display_name:'维护员',password:'new-password-123'})
 await identity.updateAdministrator('u', {display_name:'新姓名',active:false})
 await identity.deleteAdministrator('u')
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/platform/administrators?search=%E7%8E%8B+%26+admin')
 expect(fetch.mock.calls.slice(1).map(call => [call[0],call[1].method])).toEqual([
  ['/api/v1/platform/administrators','POST'],
  ['/api/v1/platform/administrators/u','PUT'],
  ['/api/v1/platform/administrators/u','DELETE'],
 ])
 expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({data:{type:'administrators',attributes:{username:'new-admin',display_name:'维护员',password:'new-password-123'}}})
 expect(JSON.parse(fetch.mock.calls[2][1].body)).toEqual({data:{type:'administrators',attributes:{display_name:'新姓名',active:false}}})
})
it('decodes administrator responses shared with the Rust serialization contract', async () => {
 const { default: fixture } = await import('../fixtures/administrator.json')
 vi.stubGlobal('fetch',vi.fn().mockResolvedValue(Response.json(fixture,{headers:{'Content-Type':'application/vnd.api+json'}})))
 const result = await identity.createAdministrator({username:'admin',display_name:'维护员',password:'new-password-123'})
 expect(result).toEqual({id:fixture.data.id,...fixture.data.attributes})
 expect(result).not.toHaveProperty('password_hash')
})
