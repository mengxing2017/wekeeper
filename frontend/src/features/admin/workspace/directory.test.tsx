import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, configure, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Directory, WecomBinding } from './directory';
import { directory } from './api';
vi.mock('./api',async importOriginal=>({...await importOriginal<typeof import('./api')>(),directory:{members:vi.fn(),departments:vi.fn(),status:vi.fn(),sync:vi.fn(),bind:vi.fn(),unbind:vi.fn()}}));
class TestResizeObserver { observe() {} unobserve() {} disconnect() {} }
vi.stubGlobal('ResizeObserver', TestResizeObserver);
configure({ asyncUtilTimeout: 5000 });
vi.setConfig({ testTimeout: 20000 });
const computedStyle = window.getComputedStyle;
window.getComputedStyle = element => computedStyle(element);
Object.defineProperty(window, 'matchMedia', { writable: true, value: (query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() }) });
afterEach(() => cleanup());
const member={id:'m',userid:'zhangsan',name:'张三',status:1,visibility:'visible' as const,verification:'verified' as const,departments:[{id:2,parentid:0,name:'运营部',order:0}],checked_at:null,bound_account_id:null,bound_account_name:null};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(directory.departments).mockResolvedValue(member.departments);vi.mocked(directory.members).mockResolvedValue({items:[member],page:{number:1,size:30,has_more:false}});});
function show(node:React.ReactNode){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{node}</QueryClientProvider>);}
it('binds a selected member using the account binding revision',async()=>{
    const refresh=vi.fn().mockResolvedValue(undefined);vi.mocked(directory.bind).mockResolvedValue(null);
    show(<WecomBinding account={{id:'a',active:true,wecom_binding_revision:7}} refresh={refresh}/>);
    fireEvent.click(await screen.findByText('选择成员'));
    fireEvent.click(screen.getByText('核验并保存绑定'));
    await waitFor(()=>expect(directory.bind).toHaveBeenCalledWith('a','m',7));
    expect(refresh).toHaveBeenCalled();
});
it('does not allow a member already bound to another account',async()=>{
    vi.mocked(directory.members).mockResolvedValue({items:[{...member,bound_account_id:'other',bound_account_name:'其他账号'}]});
    show(<WecomBinding account={{id:'a',active:true}} refresh={vi.fn()}/>);
    expect((await screen.findByText('选择成员')).closest('button')?.disabled).toBe(true);
});
it('shows sync errors and the last successful snapshot without claiming success',async()=>{
    vi.mocked(directory.status).mockResolvedValue({id:'current',configured:true,configuration_error:null,snapshot_id:'snapshot',last_success_at:'2026-10-21',latest_run:{id:'run',state:'failed',error_code:'wecom_60011',created_at:'',started_at:null,finished_at:null,department_count:0,member_count:0}});
    show(<Directory/>);
    expect((await screen.findByText(/同步失败/)).textContent).toContain('wecom_60011');
    expect(screen.getByText(/最近完整同步/).textContent).toContain('2026-10-21');
});
