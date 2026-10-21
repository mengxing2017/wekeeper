import { workspace, type Signature, type ClientState } from './api';
interface WecomSdk {
    register(input: {
        corpId: string;
        agentId: number;
        jsApiList: string[];
        getConfigSignature: (url: string) => Promise<Signature>;
        getAgentConfigSignature: (url: string) => Promise<Signature>;
    }): void;
    getCurExternalChat(input: Record<string, never>): Promise<{
        chatId: string;
    }>;
    sendChatMessage(input: {
        msgtype: 'text';
        text: {
            content: string;
        };
        enterChat: true;
    }): Promise<unknown>;
}
declare global {
    interface Window {
        ww?: WecomSdk;
    }
}
const SDK_URL = 'https://wwcdn.weixin.qq.com/node/wework/wwopen/js/wecom-jssdk-2.4.0.js';
let loading: Promise<void> | undefined;
let initializing: Promise<WecomSdk> | undefined;

function loadSdk() {
    if (window.ww) return Promise.resolve();
    loading ??= new Promise<void>((resolve, reject) => {
        const script = document.createElement('script');
        const fail = () => {
            clearTimeout(timer);
            script.onload = script.onerror = null;
            script.remove();
            loading = undefined;
            reject(new Error('企业微信 SDK 加载失败或超时，请检查网络后重新读取当前群'));
        };
        const timer = setTimeout(fail, 15000);
        script.src = SDK_URL;
        script.onload = () => {
            if (!window.ww) { fail(); return; }
            clearTimeout(timer);
            script.onload = script.onerror = null;
            resolve();
        };
        script.onerror = fail;
        document.head.append(script);
    });
    return loading;
}

export async function initializeSdk() {
    if (!/wxwork/i.test(navigator.userAgent))
        throw new Error('请从企业微信客户群的应用入口打开此页面');
    initializing ??= (async () => {
        await loadSdk();
        const sdk = window.ww;
        if (!sdk) throw new Error('企业微信 SDK 不可用');
        const info = await workspace.wecomInfo();
        const agentId = Number(info.agentId);
        if (!info.corpId || !/^\d+$/.test(info.agentId) || !Number.isSafeInteger(agentId) || agentId <= 0 || agentId > 4294967295)
            throw new Error('企业微信应用配置不完整，请联系管理员');
        sdk.register({ ...info, agentId, jsApiList: ['getCurExternalChat', 'sendChatMessage'],
            getConfigSignature: url => workspace.signature(url.split('#')[0], false),
            getAgentConfigSignature: url => workspace.signature(url.split('#')[0], true) });
        return sdk;
    })().catch(error => { initializing = undefined; throw error; });
    return initializing;
}

export async function currentChat() {
    const sdk = await initializeSdk();
    const chat = await sdk.getCurExternalChat({});
    if (!chat.chatId) throw new Error('无法读取当前客户群，请检查打开入口和应用权限');
    return chat.chatId;
}

export async function sendText(content: string) {
    if (!window.ww) throw new Error('企业微信 SDK 尚未初始化');
    await window.ww.sendChatMessage({ msgtype: 'text', text: { content }, enterChat: true });
}

// A timeout is an unknown outcome, never permission to call the SDK again.
// Late resolutions cannot change the settled outcome or trigger another send.
export async function sendOutcome(send: () => Promise<unknown>, timeoutMs = 90000): Promise<ClientState> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
        await Promise.race([
            Promise.resolve().then(send),
            new Promise<never>((_, reject) => {
                timer = setTimeout(() => reject(new Error('timeout')), timeoutMs);
            }),
        ]);
        return 'client_reported_success';
    } catch (error) {
        const message = typeof error === 'object' && error !== null && 'errMsg' in error
            ? String(error.errMsg) : '';
        if (/^sendChatMessage:cancel(?:\b|$)/i.test(message)) return 'cancelled';
        if (/^sendChatMessage:fail(?:\b|$)/i.test(message)) return 'client_reported_failure';
        return 'unknown';
    } finally {
        clearTimeout(timer);
    }
}
