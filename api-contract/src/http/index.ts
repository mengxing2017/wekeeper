/** Shared JSON:API transport, resource decoding and error handling. */
export interface ApiFailure {
  errors: Array<{ status?: string; code?: string; title?: string; detail?: string; meta?: Record<string, unknown> }>
}

function failureDetails(value: unknown): { code: string; message: string; meta?: Record<string, unknown> } {
  if (typeof value === 'object' && value !== null && 'errors' in value && Array.isArray(value.errors)) {
    const error: unknown = value.errors[0]
    if (typeof error === 'object' && error !== null) {
      return {
        meta: 'meta' in error && typeof error.meta === 'object' && error.meta !== null ? error.meta as Record<string, unknown> : undefined,
        code: 'code' in error && typeof error.code === 'string' ? error.code : 'http_error',
        message: 'detail' in error && typeof error.detail === 'string' ? error.detail
          : 'title' in error && typeof error.title === 'string' ? error.title : '服务暂时不可用',
      }
    }
  }
  return { code: 'http_error', message: '服务暂时不可用' }
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string, public readonly meta?: Record<string, unknown>) {
    super(message)
    this.name = 'ApiError'
  }
}

let csrf = ''
export function setCsrf(value: string) { csrf = value }
export async function requestJson<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal, extra?: Record<string, string>): Promise<T> {
  if (!path.startsWith('/') || path.startsWith('//') || /[\\\u0000-\u0020]/.test(path)) throw new Error('API path must be same-origin')
  const payload = body === undefined ? undefined : body instanceof Blob ? body : JSON.stringify(path.startsWith('/api/') ? writeDocument(path, body) : body)
  let response: Response
  try {
    response = await fetch(path, {
      method, credentials: 'same-origin', signal,
      headers: { Accept: path.startsWith('/api/') ? 'application/vnd.api+json' : 'application/json', ...(body instanceof Blob ? {} : { 'Content-Type': path.startsWith('/api/') ? 'application/vnd.api+json' : 'application/json' }), ...(csrf ? { 'X-CSRF-Token': csrf } : {}), ...extra },
      body: payload,
    })
  } catch (error) {
    if (signal?.aborted) throw error
    throw new ApiError(0, 'network_error', '网络中断，请刷新记录核对操作结果')
  }
  if (!response.ok) {
    let failure: unknown
    try { failure = await response.json() } catch { /* gateway response */ }
    const { code, message, meta } = failureDetails(failure)
    throw new ApiError(response.status, code, message, meta)
  }
  if (path.startsWith('/api/') && response.headers.get('Content-Type')?.split(';')[0] !== 'application/vnd.api+json') return invalidDocument()
  try { return await response.json() as T } catch { return invalidDocument() }
}
export function getJson<T>(path: string, signal?: AbortSignal): Promise<T> { return requestJson(path, 'GET', undefined, signal) }

export interface ResourceObject {
  type: string
  id: string
  attributes: Record<string, unknown>
}
export interface PageMeta { number: number; size: number; has_more: boolean }
export interface Collection<T> { items: T[]; page?: PageMeta }
export interface ResourceDocument {
  data: ResourceObject | ResourceObject[] | null
  meta?: { page?: PageMeta; [key: string]: unknown }
}
function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
function invalidDocument(): never { throw new ApiError(200, 'invalid_document', '服务返回了不合法的 JSON:API 文档') }
function decodeResource(value: unknown): Record<string, unknown> {
  if (!object(value) || typeof value.type !== 'string' || !value.type || typeof value.id !== 'string' || !value.id || !object(value.attributes) || 'id' in value.attributes || 'type' in value.attributes) return invalidDocument()
  return { ...value.attributes, id: value.id }
}
export async function requestDocument(path: string, method = 'GET', body?: unknown, signal?: AbortSignal, headers?: Record<string, string>): Promise<ResourceDocument> {
  const value = await requestJson<unknown>(path, method, body, signal, headers)
  if (!object(value) || !('data' in value) || 'errors' in value || (value.meta !== undefined && !object(value.meta))) return invalidDocument()
  if (Array.isArray(value.data)) value.data.forEach(decodeResource)
  else if (value.data !== null) decodeResource(value.data)
  const page = object(value.meta) ? value.meta.page : undefined
  if (page !== undefined && (!object(page) || !Number.isInteger(page.number) || Number(page.number) < 1 || !Number.isInteger(page.size) || Number(page.size) < 1 || typeof page.has_more !== 'boolean')) return invalidDocument()
  return value as unknown as ResourceDocument
}
export function decodeData<T>(document: ResourceDocument): T {
  return (Array.isArray(document.data) ? document.data.map(decodeResource) : document.data === null ? null : decodeResource(document.data)) as T
}
export async function requestResource<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal, headers?: Record<string, string>): Promise<T> {
  return decodeData<T>(await requestDocument(path, method, body, signal, headers))
}
export async function requestCollection<T>(path: string, signal?: AbortSignal): Promise<Collection<T>> {
  const document = await requestDocument(path, 'GET', undefined, signal)
  if (!Array.isArray(document.data)) return invalidDocument()
  return { items: decodeData<T[]>(document), page: document.meta?.page }
}

/** Explicit aliases for action routes whose target resource differs from the URL leaf. */
export function resourceType(path: string): string {
  const parts = path.split('?')[0].split('/').filter(Boolean)
  if (parts[0] !== 'api' || parts[1] !== 'v1') throw new Error('Unknown resource path')
  if (parts[2] === 'platform' && parts[3] === 'wecom-assets' && parts[4] === 'owner-scope-decisions') return 'wecom-owner-scope-decisions'
  if (parts[2] === 'platform' && parts[3] === 'wecom-assets' && parts[4] === 'sync-runs') return 'wecom-asset-sync-runs'
  if (parts[2] === 'schools' && parts[4] === 'groups' && parts[5] === 'from-directory') return 'wecom-group-bindings'
  if (parts[2] === 'platform' && parts[3] === 'execution-devices') {
    if (parts[5] === 'enrollments') return 'device-enrollments'
    if (parts[5] === 'account-bindings') return 'device-account-bindings'
    return 'execution-devices'
  }
  if (parts[2] === 'platform' && parts[3] === 'sending-accounts') return 'sending-accounts'
  if (parts[2] === 'schools' && parts[4] === 'delivery-groups') return 'delivery-groups'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-settings') return 'mobile-delivery-settings'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-tasks') {
    if (parts[6] === 'previews') return 'mobile-delivery-previews'
    if (parts[6] === 'items') return 'mobile-delivery-items'
    if (parts[6] === 'events') return 'mobile-delivery-events'
    return 'mobile-delivery-tasks'
  }
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-items') return parts[6] === 'resend-drafts' ? 'mobile-delivery-tasks' : 'mobile-delivery-items'
  if (parts[2] === 'auth') return 'sessions'
  if (parts[2] === 'platform' && parts[3] === 'wecom-directory' && parts[4] === 'credential-rotations') return 'wecom-credential-rotations'
  if (parts[2] === 'accounts' && parts[4] === 'wecom-binding') return 'wecom-bindings'
  if (parts[2] === 'profile' || parts[2] === 'accounts') return 'accounts'
  if (parts[2] === 'platform' && parts[3] === 'resources') return 'resources'
  if (parts[2] === 'schools') {
    const kind = parts[4] ?? 'schools'
    if (kind === 'members') return 'accounts'
    if (kind === 'posts' && parts[6] === 'comments') return 'comments'
    if (kind === 'templates' && parts[6] === 'versions') return 'template-versions'
    if (kind === 'delivery-drafts' && parts[6] === 'attempts') return 'delivery-attempts'
    if (['broadcast-tasks','schools','posts','comments','cases','resources','groups','templates','template-versions','delivery-drafts','delivery-attempts'].includes(kind)) return kind
  }
  throw new Error('Unknown write resource path')
}
function writeDocument(path: string, body: unknown) {
  if (!object(body)) throw new Error('Resource attributes must be an object')
  const {id, ...attributes} = body
  if ('type' in attributes || (id !== undefined && (typeof id !== 'string' || !id))) throw new Error('Invalid resource identity')
  return {data:{type:resourceType(path), ...(id === undefined ? {} : {id}), attributes}}
}
