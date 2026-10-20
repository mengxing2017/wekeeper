import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ShareWorkspace } from './index'
import { identity, workspace } from './api'
import { currentChat, sendText } from './sdk'
vi.mock('./api',async importOriginal=>({...await importOriginal<typeof import('./api')>(),identity:{session:vi.fn()},workspace:{wecomStatus:vi.fn(),schools:vi.fn(),list:vi.fn(),prepare:vi.fn(),claim:vi.fn(),result:vi.fn()}}))
vi.mock('./sdk',async importOriginal=>({...await importOriginal<typeof import('./sdk')>(),currentChat:vi.fn(),sendText:vi.fn()}))
beforeEach(()=>{
 vi.spyOn(navigator,'userAgent','get').mockReturnValue('wxwork');
 vi.mocked(workspace.wecomStatus).mockResolvedValue({configured:true,oauth_available:true});
})
afterEach(()=>{vi.restoreAllMocks();cleanup();vi.resetAllMocks();localStorage.clear();sessionStorage.clear()})
function renderPage(){const c=new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}});return render(<QueryClientProvider client={c}><ShareWorkspace/></QueryClientProvider>)}
it('requires enterprise identity before offering a send action',async()=>{
 vi.mocked(identity.session).mockRejectedValue(new Error('unauthorized'));renderPage();expect(await screen.findByText('企业微信授权登录')).toBeTruthy();expect(screen.queryByText('确认正文并在当前群分享')).toBeNull()
})
it('does not invoke the SDK twice when reporting the first result fails',async()=>{
 vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'u',display_name:'Operator',platform_admin:false,bound_school_id:'s',grants:[{school_id:'s',role:'operator'}],wecom_authenticated:true}})
 vi.mocked(workspace.schools).mockResolvedValue([{id:'s',name:'School',active:true,revision:1,config:{}}])
 vi.mocked(workspace.list).mockImplementation(async(_s,kind)=>kind==='delivery-drafts'?[{id:'d',school_id:'s',group_id:'g',template_version_id:'v',content:'Frozen text',content_hash:'h',state:'ready',chat_id:'chat',group_name:'Target'}]:[])
 vi.mocked(currentChat).mockResolvedValue('chat');vi.mocked(workspace.prepare).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'prepared',reconciliation:null,client_result:null});vi.mocked(workspace.claim).mockResolvedValue({});vi.mocked(sendText).mockResolvedValue(undefined);vi.mocked(workspace.result).mockRejectedValue(new Error('result lost'))
 renderPage();await screen.findByText('Target · Frozen text');fireEvent.change(screen.getAllByRole('combobox')[1],{target:{value:'d'}});fireEvent.click(screen.getByText('读取并核对当前客户群'));await screen.findByText('当前群：chat');fireEvent.click(screen.getByText('确认正文并在当前群分享'));await waitFor(()=>expect(sendText).toHaveBeenCalledTimes(1));await screen.findByRole('alert');fireEvent.click(screen.getByText('确认正文并在当前群分享'));expect(sendText).toHaveBeenCalledTimes(1);expect(workspace.prepare).toHaveBeenCalledTimes(1)
})

function setupShare() {
 vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'u',display_name:'Operator',platform_admin:false,bound_school_id:'s',grants:[{school_id:'s',role:'operator'}],wecom_authenticated:true}})
 vi.mocked(workspace.schools).mockResolvedValue([{id:'s',name:'School',active:true,revision:1,config:{}}])
 vi.mocked(workspace.list).mockImplementation(async(_s,kind)=>kind==='delivery-drafts'?[{id:'d',school_id:'s',group_id:'g',template_version_id:'v',content:'Frozen text',content_hash:'h',state:'ready',chat_id:'chat',group_name:'Target'}]:[])
 vi.mocked(currentChat).mockResolvedValue('chat');vi.mocked(workspace.prepare).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'prepared',reconciliation:null,client_result:null});vi.mocked(workspace.claim).mockResolvedValue({});vi.mocked(sendText).mockResolvedValue(undefined);vi.mocked(workspace.result).mockResolvedValue({id:'attempt',school_id:'s',draft_id:'d',actor_id:'u',state:'client_reported_success',reconciliation:null,client_result:null})

}
async function selectAndCheck() {
 renderPage();await screen.findByText('Target · Frozen text');fireEvent.change(screen.getAllByRole('combobox')[1],{target:{value:'d'}});fireEvent.click(screen.getByText('读取并核对当前客户群'));await screen.findByText('当前群：chat');
}
it('refuses sharing when the current chat changes after preview', async()=>{
 setupShare();await selectAndCheck();vi.mocked(currentChat).mockResolvedValue('another-chat');
 fireEvent.click(screen.getByText('确认正文并在当前群分享'));await screen.findByRole('alert');
 expect(workspace.prepare).not.toHaveBeenCalled();expect(sendText).not.toHaveBeenCalled();
})
it('records cancellation without automatically retrying', async()=>{
 setupShare();vi.mocked(sendText).mockRejectedValue({errMsg:'sendChatMessage:cancel'});await selectAndCheck();
 fireEvent.click(screen.getByText('确认正文并在当前群分享'));
 await waitFor(()=>expect(workspace.result).toHaveBeenCalledWith('s','attempt','cancelled'));
 expect(sendText).toHaveBeenCalledTimes(1);
})
it('never invokes the SDK if the invocation claim response is lost', async()=>{
 setupShare();vi.mocked(workspace.claim).mockRejectedValue(new Error('claim response lost'));await selectAndCheck();
 fireEvent.click(screen.getByText('确认正文并在当前群分享'));await screen.findByRole('alert');
 expect(sendText).not.toHaveBeenCalled();expect(localStorage.getItem('wekeeper.invoked:attempt')).toBe('true');
})

it('renders configuration guidance when enterprise credentials are absent', async()=>{
 vi.mocked(workspace.wecomStatus).mockResolvedValue({configured:false,oauth_available:false});
 vi.mocked(identity.session).mockRejectedValue(Object.assign(new Error('请先登录'),{status:401}));
 renderPage();expect(await screen.findByText('企业微信尚未配置')).toBeTruthy();
 expect(screen.getByText('登录运营后台')).toBeTruthy();expect(screen.queryByText('企业微信授权登录')).toBeNull();
})
it('allows ordinary browser preview but disables SDK sending', async()=>{
 setupShare();vi.spyOn(navigator,'userAgent','get').mockReturnValue('Mozilla');renderPage();
 await screen.findByText('Target · Frozen text');fireEvent.change(screen.getAllByRole('combobox')[1],{target:{value:'d'}});
 expect(screen.getByText('当前为普通浏览器')).toBeTruthy();
 expect((screen.getByText('确认正文并在当前群分享') as HTMLButtonElement).disabled).toBe(true);
 expect(sendText).not.toHaveBeenCalled();
})
it('shows recoverable errors when configuration and session requests fail',async()=>{
 vi.mocked(workspace.wecomStatus).mockRejectedValue(new Error('offline'));
 vi.mocked(identity.session).mockRejectedValue(new Error('offline'));renderPage();
 expect(await screen.findByText('重新检查配置')).toBeTruthy();expect(await screen.findByText('重新查询身份')).toBeTruthy();
})
