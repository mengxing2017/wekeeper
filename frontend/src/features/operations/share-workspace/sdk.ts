import { workspace, type Signature } from './api';
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
let loading: Promise<void> | undefined;
export async function initializeSdk() {
    if (!/wxwork/i.test(navigator.userAgent))
        throw new Error('请从企业微信客户群的应用入口打开此页面');
    if (!window.ww) {
        loading ??= new Promise<void>((resolve, reject) => { const script = document.createElement('script'); script.src = 'https://wwcdn.weixin.qq.com/node/wework/wwopen/js/wecom-jssdk-2.4.0.js'; script.onload = () => resolve(); script.onerror = () => { loading = undefined; reject(new Error('企业微信 SDK 加载失败')); }; document.head.append(script); });
        await loading;
    }
    const sdk = window.ww;
    if (!sdk)
        throw new Error('企业微信 SDK 不可用');
    const info = await workspace.wecomInfo();
    sdk.register({ ...info, agentId: Number(info.agentId), jsApiList: ['getCurExternalChat', 'sendChatMessage'], getConfigSignature: url => workspace.signature(url, false), getAgentConfigSignature: url => workspace.signature(url, true) });
    return sdk;
}
export async function currentChat() { const sdk = await initializeSdk(); const chat = await sdk.getCurExternalChat({}); if (!chat.chatId)
    throw new Error('无法读取当前客户群，请检查打开入口和应用权限'); return chat.chatId; }
export async function sendText(content: string) { if (!window.ww)
    throw new Error('企业微信 SDK 尚未初始化'); await window.ww.sendChatMessage({ msgtype: 'text', text: { content }, enterChat: true }); }
