import { formatDate } from '../../../shared/format-date';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import {cleanup,configure,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {Assets,GroupPicker} from './assets';
import {assets,directory,type AssetStatus} from './api';
vi.mock('./api',async original=>({...await original<typeof import('./api')>(),assets:{status:vi.fn(),sync:vi.fn(),list:vi.fn(),detail:vi.fn(),relations:vi.fn(),candidates:vi.fn(),bind:vi.fn()},directory:{members:vi.fn(),status:vi.fn(),sync:vi.fn(),run:vi.fn()}}));
class Observer {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Observer);configure({asyncUtilTimeout:5000});vi.setConfig({testTimeout:20000});
const computedStyle=window.getComputedStyle;window.getComputedStyle=e=>computedStyle(e);
Object.defineProperty(window,'matchMedia',{writable:true,value:(q:string)=>({matches:false,media:q,onchange:null,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn(),dispatchEvent:vi.fn()})});
afterEach(cleanup);
const status:AssetStatus={server_configured:true,configuration_error:null,credential_generation:1,datasets:[{dataset:'groups',current_run_id:'previous',last_success_at:'2026-10-21 12:00',published_generation:1,freshness:'unverified',scope_changed:false,scope_count:0,latest_run:{id:'g',dataset:'groups',generation:1,state:'failed',stage:'collect',processed:0,total:0,pages:0,error_code:'wecom_60020',complete:false,scope_changed:false,created_at:'',started_at:null,finished_at:null}}]};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(assets.status).mockResolvedValue(status);vi.mocked(assets.list).mockResolvedValue({items:[],page:{number:1,size:30,has_more:false}});vi.mocked(directory.members).mockResolvedValue({items:[]});vi.mocked(assets.sync).mockResolvedValue([]);vi.mocked(directory.status).mockResolvedValue({id:'directory',server_configured:true,configuration_error:null,credential_generation:1,credential_rotation_required:false,snapshot_id:'snapshot',last_success_at:null,latest_run:null});});
function show(node:React.ReactNode){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{node}</QueryClientProvider>);}
it('keeps the previous snapshot visible after group failure and allows server-only sync',async()=>{
 show(<Assets/>);expect(screen.queryByRole('tab',{name:'客户'})).toBeNull();expect(screen.queryByRole('tab',{name:'客户联系成员'})).toBeNull();expect(await screen.findByText(/同步失败：wecom_60020/)).toBeTruthy();expect(screen.getByText(text => text.includes(formatDate('2026-10-21 12:00')))).toBeTruthy();fireEvent.click(screen.getByText('同步客户群'));await waitFor(()=>expect(assets.sync).toHaveBeenCalledWith('groups'));
});
it('paginates group assets using the selected resource',async()=>{
 vi.mocked(assets.list).mockResolvedValue({items:[],page:{number:1,size:30,has_more:true}});show(<Assets/>);await waitFor(()=>expect((screen.getByRole('button',{name:'下一页'}) as HTMLButtonElement).disabled).toBe(false));fireEvent.click(screen.getByRole('button',{name:'下一页'}));await waitFor(()=>expect(assets.list).toHaveBeenCalledWith('groups',{page:2}));
});
it('saves only a selected candidate and its generation/version; refreshes after conflict',async()=>{
 const candidate={already_associated:false,id:'directory',name:'官方群',chat_id:'CHAT',corp_id:'corp',owner:'Alice',owner_name:'张三',member_count:3,credential_generation:2,checked_at:'2026-10-21T00:00:00Z'};
 vi.mocked(assets.candidates).mockResolvedValue({items:[candidate]});vi.mocked(assets.bind).mockRejectedValue(new Error('数据已变化'));const refresh=vi.fn();show(<GroupPicker school="school-a" onSaved={refresh}/>);
 fireEvent.click(await screen.findByText('选择群'));fireEvent.change(screen.getByLabelText('学校群显示名'),{target:{value:'本校显示名'}});fireEvent.click(screen.getByText('关联到当前学校'));await waitFor(()=>expect(assets.bind).toHaveBeenCalledWith('school-a',{directory_group_id:'directory',expected_generation:2,expected_checked_at:candidate.checked_at,name:'本校显示名'}));expect(await screen.findByText('数据已变化')).toBeTruthy();expect(refresh).not.toHaveBeenCalled();await waitFor(()=>expect(screen.queryByText('关联到当前学校')).toBeNull());
});
it('does not expose a selection on candidate permission failure',async()=>{
 vi.mocked(assets.candidates).mockRejectedValue(new Error('请先绑定企业成员'));show(<GroupPicker school="school-a" onSaved={vi.fn()}/>);expect(await screen.findByText('请先绑定企业成员')).toBeTruthy();expect(screen.queryByText('选择群')).toBeNull();expect(assets.list).not.toHaveBeenCalled();
});

it('bootstraps the directory before the first group sync', async () => {
 const existing = await directory.status();
 vi.mocked(directory.status).mockResolvedValue({...existing, snapshot_id:null});
 vi.mocked(directory.sync).mockResolvedValue({id:'first-directory',state:'running',created_at:'',started_at:null,finished_at:null,error_code:null,department_count:0,member_count:0});
 vi.mocked(directory.run).mockResolvedValue({id:'first-directory',state:'succeeded',created_at:'',started_at:null,finished_at:null,error_code:null,department_count:1,member_count:2});
 vi.mocked(assets.status).mockResolvedValue({...status,datasets:status.datasets.map(d=>({...d,current_run_id:null,last_success_at:null}))});
 show(<Assets/>);
 expect(await screen.findByText(/尚无成功快照/)).toBeTruthy();
 fireEvent.click(screen.getByText('同步客户群'));
 expect(await screen.findByText('正在同步内部通讯录')).toBeTruthy();
 expect(assets.sync).not.toHaveBeenCalled();
 await waitFor(()=>expect(assets.sync).toHaveBeenCalledWith('groups'));
 expect(directory.run).toHaveBeenCalledWith('first-directory');
});
it('does not sync groups when the prerequisite directory sync fails', async () => {
 const existing = await directory.status();
 vi.mocked(directory.status).mockResolvedValue({...existing, snapshot_id:null});
 vi.mocked(directory.sync).mockResolvedValue({id:'failed-directory',state:'failed',created_at:'',started_at:null,finished_at:null,error_code:'wecom_60020',department_count:0,member_count:0});
 show(<Assets/>);
 await screen.findByText(/保留上次成功快照/);
 fireEvent.click(screen.getByText('同步客户群'));
 expect(await screen.findByText(/内部通讯录同步失败：wecom_60020/)).toBeTruthy();
 expect(assets.sync).not.toHaveBeenCalled();
});
it('refreshes the directory when retrying directory_snapshot_required with an old snapshot', async () => {
 vi.mocked(assets.status).mockResolvedValue({...status,datasets:status.datasets.map(d=>({...d,latest_run:d.latest_run ? {...d.latest_run,error_code:'directory_snapshot_required'} : null}))});
 vi.mocked(directory.sync).mockResolvedValue({id:'new-directory',state:'succeeded',created_at:'',started_at:null,finished_at:null,error_code:null,department_count:1,member_count:2});
 show(<Assets/>);
 await screen.findByText(/请先完成当前凭据下的内部通讯录同步/);
 fireEvent.click(screen.getByText('同步客户群'));
 await waitFor(()=>expect(assets.sync).toHaveBeenCalledWith('groups'));
 expect(directory.sync).toHaveBeenCalledTimes(1);
});
