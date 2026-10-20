import { afterEach, expect, it, vi } from 'vitest'
vi.mock('./api', () => ({ workspace: { wecomInfo: vi.fn(), signature: vi.fn() } }))
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.resetModules(); delete window.ww; document.head.innerHTML = '' })

it.each([
    [{ errMsg: 'sendChatMessage:cancel' }, 'cancelled'],
    [{ errMsg: 'sendChatMessage:fail no permission' }, 'client_reported_failure'],
    [null, 'unknown'],
    [new Error('network failure'), 'unknown'],
])('classifies SDK outcomes conservatively: %j', async (error, expected) => {
    const { sendOutcome } = await import('./sdk')
    expect(await sendOutcome(() => Promise.reject(error))).toBe(expected)
})

it('timeout settles unknown and a late SDK resolution does not send again', async () => {
    vi.useFakeTimers()
    const { sendOutcome } = await import('./sdk')
    let resolve!: () => void
    const send = vi.fn(() => new Promise<void>(done => { resolve = done }))
    const outcome = sendOutcome(send, 100)
    await vi.advanceTimersByTimeAsync(100)
    expect(await outcome).toBe('unknown')
    resolve()
    await Promise.resolve()
    expect(send).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
})

it('registers once, strips fragment for signatures and reads the current chat each time', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('wxwork')
    const { workspace } = await import('./api')
    vi.mocked(workspace.wecomInfo).mockResolvedValue({ corpId: 'corp', agentId: '1000001' })
    const register = vi.fn()
    window.ww = { register, getCurExternalChat: vi.fn().mockResolvedValue({ chatId: 'chat' }), sendChatMessage: vi.fn() }
    const { currentChat } = await import('./sdk')
    expect(await currentChat()).toBe('chat')
    expect(await currentChat()).toBe('chat')
    expect(register).toHaveBeenCalledTimes(1)
    expect(window.ww.getCurExternalChat).toHaveBeenCalledTimes(2)
    await register.mock.calls[0][0].getConfigSignature('https://example.com/wecom?school=s#fragment')
    expect(workspace.signature).toHaveBeenCalledWith('https://example.com/wecom?school=s', false)
})

it('SDK load timeout removes the script and allows an explicit retry', async () => {
    vi.useFakeTimers()
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('wxwork')
    const { initializeSdk } = await import('./sdk')
    const failed = expect(initializeSdk()).rejects.toThrow('超时')
    await vi.advanceTimersByTimeAsync(15000)
    await failed
    expect(document.head.querySelector('script')).toBeNull()
    const retried = expect(initializeSdk()).rejects.toThrow('加载失败')
    document.head.querySelector('script')!.dispatchEvent(new Event('error'))
    await retried
})
