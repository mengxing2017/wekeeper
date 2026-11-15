import {expect,it,vi} from 'vitest'
import {createClient} from '../src/http/client'
import {createKeywordRulesClient,type SensitiveWordsUpdate} from '../src/modules/source-access/keywords-client'
import fixture from '../fixtures/source-keywords.json'
it('shares sensitive word update fields with Rust fixtures',()=>{
 const publication:SensitiveWordsUpdate=fixture.update
 expect(publication.entries[0].enabled).toBe(true)
})
it('uses the sensitive words endpoint and JSON API type with CSRF',async()=>{
 const transport=vi.fn().mockResolvedValue({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'sensitive-words',id:'r',attributes:{revision:2}}}})
 const client=createClient(transport);client.setCsrf('csrf-test');const rules=createKeywordRulesClient(client)
 await rules.save(fixture.update)
 expect(transport.mock.calls[0][0].headers['X-CSRF-Token']).toBe('csrf-test')
 expect(JSON.parse(transport.mock.calls[0][0].body)).toEqual({data:{type:'sensitive-words',attributes:fixture.update}})
})
