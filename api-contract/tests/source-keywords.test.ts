import {expect,it,vi} from 'vitest'
import {createClient} from '../src/http/client'
import {createKeywordRulesClient,type KeywordCheck,type KeywordPublication} from '../src/modules/source-access/keywords-client'
import fixture from '../fixtures/source-keywords.json'
it('shares immutable evidence and publication fields with Rust fixtures',()=>{
 const publication:KeywordPublication=fixture.publication
 const check=fixture.check as KeywordCheck
 expect(publication.entries[0].enabled).toBe(true)
 expect(check.context.versions[0].revision).toBe(2)
 expect(check.algorithm).toBe('nfkc-fullfold/1')
 expect(check.truncated).toBe(true)
})
it('uses scoped neutral clients and the publication JSON API type with CSRF',async()=>{
 const transport=vi.fn().mockResolvedValue({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'keyword-rule-publications',id:'r',attributes:{revision:2}}}})
 const client=createClient(transport);client.setCsrf('csrf-test');const rules=createKeywordRulesClient(client)
 await rules.publish('mobile:workspace',fixture.publication)
 expect(transport.mock.calls[0][0].headers['X-CSRF-Token']).toBe('csrf-test')
 expect(JSON.parse(transport.mock.calls[0][0].body)).toEqual({data:{type:'keyword-rule-publications',attributes:fixture.publication}})
 await rules.hits('route-one','receipt-one','check-one',2)
 expect(transport.mock.calls[1][0].path).toBe('/api/v2/admin/source-routes/route-one/receipts/receipt-one/checks/check-one/hits?page=2')
})
