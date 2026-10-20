export interface ApiFailure {
  error: { code: string; message: string }
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message)
    this.name = 'ApiError'
  }
}

let csrf = ''
export function setCsrf(value: string) { csrf = value }
export async function requestJson<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal, extra?: Record<string, string>): Promise<T> {
  if (!path.startsWith('/') || path.startsWith('//') || /[\\\u0000-\u0020]/.test(path)) throw new Error('API path must be same-origin')
  let response: Response
  try {
    response = await fetch(path, {
      method, credentials: 'same-origin', signal,
      headers: { Accept: 'application/json', ...(body instanceof Blob ? {} : { 'Content-Type': 'application/json' }), ...(csrf ? { 'X-CSRF-Token': csrf } : {}), ...extra },
      body: body === undefined ? undefined : body instanceof Blob ? body : JSON.stringify(body),
    })
  } catch (error) {
    if (signal?.aborted) throw error
    throw new ApiError(0, 'network_error', '网络中断，请刷新记录核对操作结果')
  }
  if (!response.ok) {
    let failure: ApiFailure | undefined
    try { failure = await response.json() as ApiFailure } catch { /* gateway response */ }
    throw new ApiError(response.status, failure?.error?.code ?? 'http_error', failure?.error?.message ?? '服务暂时不可用')
  }
  return response.json() as Promise<T>
}
export function getJson<T>(path: string, signal?: AbortSignal): Promise<T> { return requestJson(path, 'GET', undefined, signal) }
