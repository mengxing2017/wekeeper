import {expect,it,vi} from 'vitest'
import {createClient} from '../src/http/client'
import {createAdvertisingClient,type AdSelection,type AdConfig,type AdScope,type AdContent,type AdPolicy} from '../src/modules/advertising/client'
import fixture from '../fixtures/source-advertising.json'
it('shares advertising publication and frozen selection fields with the Rust serialization fixture',()=>{
 const config:AdConfig=fixture.configuration.config as AdConfig
 const scope:AdScope=fixture.create.workspaces[0] as AdScope
 const content:AdContent=fixture.content_publication.content
 const policy:AdPolicy=fixture.policy_publication.policy as AdPolicy
 const selection:AdSelection=fixture.selection as AdSelection
 expect(config.mode).toBe('fixed');expect(scope.channel).toBe('mobile');expect(content.link).toMatch(/^https:/);expect(policy.allows[0]).toEqual(scope);expect(selection.composer).toBe('source-ad-text/1')
})
it('writes typed JSON API publications with CSRF and encodes route IDs',async()=>{
 const transport=vi.fn().mockResolvedValue({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'advertisements',id:'a',attributes:{revision:2}}}})
 const c=createClient(transport);c.setCsrf('csrf-ad');const ads=createAdvertisingClient(c)
 await ads.create(fixture.create as Parameters<typeof ads.create>[0])
 expect(JSON.parse(transport.mock.calls[0][0].body)).toEqual({data:{type:'advertisements',attributes:fixture.create}})
 expect(transport.mock.calls[0][0].headers['X-CSRF-Token']).toBe('csrf-ad')
 await ads.configure('route a',0,fixture.configuration.config as AdConfig,'选用广告')
 expect(transport.mock.calls[1][0].path).toBe('/api/v2/admin/source-routes/route%20a/advertisement-configuration')
 expect(JSON.parse(transport.mock.calls[1][0].body).data.type).toBe('advertisement-configurations')
 await ads.preview('route a',{title:null,content:'正文',detail_url:'https://example.com'})
 expect(JSON.parse(transport.mock.calls[2][0].body).data.type).toBe('composition-previews')
 await ads.policy('a',2,fixture.policy_publication.policy as AdPolicy,'共享')
 expect(JSON.parse(transport.mock.calls[3][0].body).data.type).toBe('advertisement-policy-publications')
})

it('requests archived advertisements explicitly instead of filtering a paginated active list',async()=>{
 const requestResource=vi.fn().mockResolvedValue({items:[],has_more:false})
 const ads=createAdvertisingClient({requestResource})
 await ads.list(2,'archived')
 expect(requestResource).toHaveBeenCalledWith('/api/v2/admin/advertisements?page=2&filter=archived')
})
