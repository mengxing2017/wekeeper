import { formatDate } from '../../../shared/format-date';
import { selectOption } from '../../../test/select-option'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ShareWorkspace } from './index'
import { identity, workspace } from './api'
import { currentChat, sendText } from './sdk'
vi.mock('./api',async importOriginal=>({...await importOriginal<typeof import('./api')>(),identity:{session:vi.fn()},workspace:{wecomStatus:vi.fn(),schools:vi.fn(),list:vi.fn(),getDraft:vi.fn(),relatedHistory:vi.fn(),prepare:vi.fn(),claim:vi.fn(),result:vi.fn()}}))
vi.mock('./sdk',async importOriginal=>({...await importOriginal<typeof import('./sdk')>(),currentChat:vi.fn(),sendText:vi.fn()}))
beforeEach(()=>{
 vi.spyOn(navigator,'userAgent','get').mockReturnValue('wxwork');
 vi.mocked(workspace.wecomStatus).mockResolvedValue({server_configured:true,client_configured:true,oauth_available:true,share_available:true});
 vi.mocked(workspace.getDraft).mockResolvedValue({id:'d',school_id:'s',group_id:'g',template_version_id:'v',content:'Frozen text',content_hash:'h',state:'ready',chat_id:'chat',group_name:'Target'});
 vi.mocked(workspace.relatedHistory).mockResolvedValue({page:{number:1,size:30,has_more:false},items:[],latest_attempt_id:null,blocked:false});
})
afterEach(()=>{vi.restoreAllMocks();cleanup();vi.resetAllMocks();localStorage.clear();sessionStorage.clear();window.history.replaceState({},'', '/')})
function renderPage(){const c=new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}});return render(<QueryClientProvider client={c}><ShareWorkspace/></QueryClientProvider>)}
it('requires enterprise identity before offering a send action',async()=>{
 vi.mocked(identity.session).mockRejectedValue(new Error('unauthorized'));renderPage();expect(await screen.findByText('企业微信授权登录')).toBeTruthy();expect(screen.queryByText('确认正文并在当前群分享')).toBeNull()
})
it('does not invoke the SDK twice when reporting the first result fails',async()=>{
 vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'u',display_name:'Operator',platform_admin:false,bound_school_id:'s',grants:[{school_id:'s',role:'operator'}],wecom_authenticated:true}})
 vi.mocked(workspace.schools).mockResolvedValue([{id:'s',name:'School',active:true,revision:1,config:{}}])
 vi.mocked(workspace.list).mockImplementation(async(_s,kind)=>({items:kind==='delivery-drafts'?[{id:'d',school_id:'s',group_id:'g',template_version_id:'v',content:'Frozen text',content_hash:'h',state:'ready',chat_id:'chat',group_name:'Target'}]:[],page:{number:1,size:30,has_more:false}}))
 vi.mocked(currentChat).mockResolvedValue('chat');vi.mocked(workspace.prepare).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'prepared',reconciliation:null,client_result:null});vi.mocked(workspace.claim).mockResolvedValue({});vi.mocked(sendText).mockResolvedValue(undefined);vi.mocked(workspace.result).mockRejectedValue(new Error('result lost'))
 renderPage();await selectOption(await screen.findByRole('combobox', { name: '待分享草稿' }), 'Target · Frozen text');fireEvent.click(await screen.findByRole('button', { name: '读取并核对当前客户群' }));await screen.findByText('当前群：chat');fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));await waitFor(()=>expect(sendText).toHaveBeenCalledTimes(1));await screen.findByRole('alert');fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));expect(sendText).toHaveBeenCalledTimes(1);expect(workspace.prepare).toHaveBeenCalledTimes(1)
})

function setupShare() {
 vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'u',display_name:'Operator',platform_admin:false,bound_school_id:'s',grants:[{school_id:'s',role:'operator'}],wecom_authenticated:true}})
 vi.mocked(workspace.schools).mockResolvedValue([{id:'s',name:'School',active:true,revision:1,config:{}}])
 vi.mocked(workspace.list).mockImplementation(async(_s,kind)=>({items:kind==='delivery-drafts'?[{id:'d',school_id:'s',group_id:'g',template_version_id:'v',content:'Frozen text',content_hash:'h',state:'ready',chat_id:'chat',group_name:'Target'}]:[],page:{number:1,size:30,has_more:false}}))
 vi.mocked(currentChat).mockResolvedValue('chat');vi.mocked(workspace.prepare).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'prepared',reconciliation:null,client_result:null});vi.mocked(workspace.claim).mockResolvedValue({});vi.mocked(sendText).mockResolvedValue(undefined);vi.mocked(workspace.result).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'client_reported_success',reconciliation:null,client_result:null})

}
async function selectAndCheck() {
 renderPage();await selectOption(await screen.findByRole('combobox', { name: '待分享草稿' }), 'Target · Frozen text');fireEvent.click(await screen.findByRole('button', { name: '读取并核对当前客户群' }));await screen.findByText('当前群：chat');
}
it('refuses sharing when the current chat changes after preview', async()=>{
 setupShare();await selectAndCheck();vi.mocked(currentChat).mockResolvedValue('another-chat');
 fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));await screen.findByRole('alert');
 expect(workspace.prepare).not.toHaveBeenCalled();expect(sendText).not.toHaveBeenCalled();
})
it('records cancellation without automatically retrying', async()=>{
 setupShare();vi.mocked(sendText).mockRejectedValue({errMsg:'sendChatMessage:cancel'});await selectAndCheck();
 fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));
 await waitFor(()=>expect(workspace.result).toHaveBeenCalledWith('s','attempt','cancelled'));
 expect(sendText).toHaveBeenCalledTimes(1);
})
it('never invokes the SDK if the invocation claim response is lost', async()=>{
 setupShare();vi.mocked(workspace.claim).mockRejectedValue(new Error('claim response lost'));await selectAndCheck();
 fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));await screen.findByRole('alert');
 expect(sendText).not.toHaveBeenCalled();expect(localStorage.getItem('wekeeper.invoked:attempt')).toBe('true');
})

it('renders configuration guidance when enterprise credentials are absent', async()=>{
 vi.mocked(workspace.wecomStatus).mockResolvedValue({server_configured:false,client_configured:false,oauth_available:false,share_available:false});
 vi.mocked(identity.session).mockRejectedValue(Object.assign(new Error('请先登录'),{status:401}));
 renderPage();expect(await screen.findByText('企业微信尚未配置')).toBeTruthy();
 expect(screen.getByText('登录运营后台')).toBeTruthy();expect(screen.queryByText('企业微信授权登录')).toBeNull();
})
it('allows ordinary browser preview but disables SDK sending', async()=>{
 setupShare();vi.spyOn(navigator,'userAgent','get').mockReturnValue('Mozilla');renderPage();
 await selectOption(await screen.findByRole('combobox', { name: '待分享草稿' }), 'Target · Frozen text');
 expect(screen.getByText('当前为普通浏览器')).toBeTruthy();
 expect((await screen.findByRole('button', { name: '确认正文并在当前群分享' }) as HTMLButtonElement).disabled).toBe(true);
 expect(sendText).not.toHaveBeenCalled();
})
it('shows recoverable errors when configuration and session requests fail',async()=>{
 vi.mocked(workspace.wecomStatus).mockRejectedValue(new Error('offline'));
 vi.mocked(identity.session).mockRejectedValue(new Error('offline'));renderPage();
 expect(await screen.findByText('重新检查配置')).toBeTruthy();expect(await screen.findByText('重新查询身份')).toBeTruthy();
})

it('loads an older draft directly even when it is absent from the first page', async()=>{
 setupShare();window.history.replaceState({},'', '/wecom?school=s&draft=d');
 vi.mocked(workspace.list).mockResolvedValue({items:[], page:{number:1,size:30,has_more:false}});renderPage();
 expect(await screen.findByText('目标客户群：Target（chat）')).toBeTruthy();
 expect(workspace.getDraft).toHaveBeenCalledWith('s','d');
 window.history.replaceState({},'', '/');
})
it('offers history from an overlapping older draft for explicit resend', async()=>{
 setupShare();vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'u',display_name:'Admin',platform_admin:true,bound_school_id:'s',grants:[],wecom_authenticated:true}});
 vi.mocked(workspace.relatedHistory).mockResolvedValue({page:{number:1,size:30,has_more:false},blocked:false,latest_attempt_id:'old-attempt',items:[{id:'old-attempt',school_id:'s',draft_id:'different-draft',actor_id:'u',state:'client_reported_success',reconciliation:null,client_result:null,group_name:'Target',created_at:'2026-10-21'}]});
 await selectAndCheck();
 expect((screen.getByRole('button', { name: '确认正文并在当前群分享' }) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button', { name: '明确再次分享' }));
 await selectOption(await screen.findByLabelText('引用历史尝试'), `Target · ${formatDate('2026-10-21')} · 客户端报告成功 · old-attempt`);
 fireEvent.change(screen.getByLabelText('再次分享原因'),{target:{value:'核对后再次分享'}});
 fireEvent.click(screen.getByText('准备明确再次分享'));
 fireEvent.click(screen.getByRole('button', { name: '确认正文并在当前群分享' }));
 await waitFor(()=>expect(workspace.prepare).toHaveBeenCalledWith('s','d',expect.any(String),expect.objectContaining({resend_of:'old-attempt',reason:'核对后再次分享'})));
})
it('blocks sending when an older overlapping item still has an unresolved result', async()=>{
 setupShare();vi.mocked(workspace.relatedHistory).mockResolvedValue({page:{number:1,size:30,has_more:false},blocked:true,latest_attempt_id:'newer',items:[]});
 await selectAndCheck();
 expect((screen.getByRole('button', { name: '确认正文并在当前群分享' }) as HTMLButtonElement).disabled).toBe(true);
 expect(sendText).not.toHaveBeenCalled();
})
it('allows server-only OAuth and preview while disabling both SDK actions', async()=>{
 setupShare();vi.mocked(workspace.wecomStatus).mockResolvedValue({server_configured:true,client_configured:false,oauth_available:true,share_available:false});
 renderPage();await selectOption(await screen.findByRole('combobox', { name: '待分享草稿' }), 'Target · Frozen text');
 expect(screen.getByText('企业微信登录可用，客户端分享尚未配置')).toBeTruthy();
 expect((await screen.findByRole('button', { name: '读取并核对当前客户群' }) as HTMLButtonElement).disabled).toBe(true);
 expect((screen.getByRole('button', { name: '确认正文并在当前群分享' }) as HTMLButtonElement).disabled).toBe(true);
 expect(currentChat).not.toHaveBeenCalled();expect(workspace.prepare).not.toHaveBeenCalled();
});
it('keeps the OAuth link when only the client application identity is missing',async()=>{
 vi.mocked(workspace.wecomStatus).mockResolvedValue({server_configured:true,client_configured:false,oauth_available:true,share_available:false});
 vi.mocked(identity.session).mockRejectedValue(new Error('unauthorized'));renderPage();
 expect(await screen.findByText('企业微信授权登录')).toBeTruthy();
});
