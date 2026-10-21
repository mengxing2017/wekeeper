import { afterEach, expect, it, vi } from 'vitest';
import { directory, identity } from '../src/modules/business';
import fixture from '../fixtures/wecom-directory-member.json';
afterEach(()=>vi.unstubAllGlobals());
it('decodes the shared Rust directory fixture and preserves filters',async()=>{
    const fetch=vi.fn().mockResolvedValue(Response.json(fixture,{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
    const result=await directory.members({search:'张三',department_id:2,binding:'unbound',page:2});
    expect(result.items[0]).toMatchObject({userid:'Zhang.San',name:'张三',status:1,departments:[{name:'运营部'}]});
    expect(fetch.mock.calls[0][0]).toContain('page%5Bnumber%5D=2');
    expect(fetch.mock.calls[0][0]).toContain('binding=unbound');
});
it('uses a dedicated binding resource and revision header for unbinding',async()=>{
    const fetch=vi.fn().mockImplementation(()=>Promise.resolve(Response.json({data:null},{headers:{'Content-Type':'application/vnd.api+json'}})));vi.stubGlobal('fetch',fetch);
    await directory.bind('account','member',3);
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'wecom-bindings',attributes:{member_id:'member',revision:3}}});
    await directory.unbind('account',4);
    expect(fetch.mock.calls[1][1].headers['If-Match']).toBe('"4"');
    await identity.update('account',{display_name:'昵称',active:true});
    expect(JSON.parse(fetch.mock.calls[2][1].body).data.attributes).not.toHaveProperty('wecom_userid');
});
it('rotates only the current server credentials with an explicit generation',async()=>{
    const fetch=vi.fn().mockResolvedValue(Response.json({data:null},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
    await directory.rotate(3);
    expect(fetch.mock.calls[0][0]).toBe('/api/v1/platform/wecom-directory/credential-rotations');
    expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'wecom-credential-rotations',attributes:{expected_generation:3}}});
});
