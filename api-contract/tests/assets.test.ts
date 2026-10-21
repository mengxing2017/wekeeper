import {afterEach,expect,it,vi} from 'vitest';
import {assets} from '../src/modules/business';
import fixture from '../fixtures/wecom-asset-group.json';
import memberFixture from '../fixtures/wecom-asset-member.json';
afterEach(()=>vi.unstubAllGlobals());
it('decodes the real Rust/PostgreSQL group fixture and transmits member filters',async()=>{
 const fetch=vi.fn().mockResolvedValue(Response.json(fixture,{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
 const result=await assets.list('groups',{page:2,participant_member_id:'member',associated:false});
 expect(result.items[0]).toMatchObject({chat_id:'CHAT',owner:'Alice',member_count:3,member_version:'opaque-V1'});expect(result.page?.has_more).toBe(false);
 expect(fetch.mock.calls[0][0]).toContain('participant_member_id=member');expect(fetch.mock.calls[0][0]).toContain('associated=false');
});
it('group sync returns its run and uses the asset JSON API resource type',async()=>{
 const fetch=vi.fn().mockResolvedValue(Response.json({data:[{type:'wecom-asset-sync-runs',id:'g',attributes:{dataset:'groups',state:'queued'}}]},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
 const runs=await assets.sync('groups');expect(runs.map(r=>r.dataset)).toEqual(['groups']);expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'wecom-asset-sync-runs',attributes:{dataset:'groups'}}});
});
it('school association requires both credential generation and the exact observed object version',async()=>{
 const fetch=vi.fn().mockResolvedValue(Response.json({data:{type:'groups',id:'g',attributes:{}}},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
 const input={directory_group_id:'directory',expected_generation:3,expected_checked_at:'2026-10-21T00:00:00Z',name:'学校显示名'};
 await assets.bind('school',input);expect(fetch.mock.calls[0][0]).toBe('/api/v1/schools/school/groups/from-directory');expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'wecom-group-bindings',attributes:input}});
});

it('decodes group relations using member_type without a reserved type attribute',async()=>{
 const fetch=vi.fn().mockResolvedValue(Response.json(memberFixture,{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);
 const result=await assets.relations('groups','group');
 expect(result.items[0]).toMatchObject({userid:'Alice',member_type:1,member_name:'成员0'});
 expect(result.items[0]).not.toHaveProperty('type');
 expect(result.page?.has_more).toBe(false);
 expect(fetch.mock.calls[0][0]).toContain('/groups/group/members?');
});
