import type { Transport, TransportRequest } from '../features/identity/api'
import { credentials } from './credentials'
const active = new Set<{ abort(): void }>()
export function cancelRequests() {
  for (const task of active) task.abort()
  active.clear()
}
credentials.subscribe(cancelRequests)
export function trustedOrigin(value: string) {
  if (!/^https:\/\/[a-z0-9.-]+(?::\d+)?$/i.test(value))
    throw new Error('请配置有效的 HTTPS API 域名')
  return value
}
export function createUniTransport(base: string): Transport {
  // Validate only when used, so an unconfigured build can show a useful error.
  return (request: TransportRequest) =>
    new Promise((resolve, reject) => {
      const origin = trustedOrigin(base)
      const revision = credentials.revision
      if (request.signal?.aborted) {
        reject(new Error('请求已取消'))
        return
      }
      const abort = () => task.abort()
      const cleanup = () => {
        active.delete(task)
        request.signal?.removeEventListener('abort', abort)
      }
      const task = uni.request({
        url: origin + request.path,
        method: request.method as UniApp.RequestOptions['method'],
        header: {
          ...request.headers,
          ...(credentials.token ? { Authorization: `Bearer ${credentials.token}` } : {}),
        },
        data: request.body,
        responseType: request.binary ? 'arraybuffer' : 'text',
        dataType: 'text',
        timeout: 15000,
        success(response) {
          if (revision !== credentials.revision) {
            reject(new Error('会话已更新'))
            return
          }
          let body: unknown = response.data
          if (response.statusCode >= 400 && body instanceof ArrayBuffer) {
            try {
              body = decodeURIComponent(
                Array.from(
                  new Uint8Array(body),
                  (byte) => '%' + byte.toString(16).padStart(2, '0'),
                ).join(''),
              )
            } catch {
              body = undefined
            }
          }
          if (!request.binary || response.statusCode >= 400) {
            if (typeof body === 'string') {
              try {
                body = JSON.parse(body)
              } catch {
                body = undefined
              }
            }
          }
          if (response.statusCode === 401 && credentials.token) {
            cleanup()
            credentials.set('')
          }
          resolve({
            status: response.statusCode,
            headers: response.header as Record<string, string>,
            body,
          })
        },
        fail: reject,
        complete: cleanup,
      })
      active.add(task)
      request.signal?.addEventListener('abort', abort, { once: true })
    })
}
