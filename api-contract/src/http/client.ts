import {
  ApiError,
  failureDetails,
  invalidDocument,
  object,
  decodeResource,
  writeDocument,
  type ResourceDocument,
  type Collection,
} from './protocol'
export * from './protocol'

/** Structural cancellation contract: no DOM runtime is required. */
export interface Cancellation {
  readonly aborted: boolean
  addEventListener(type: 'abort', callback: () => void, options?: { once?: boolean }): void
  removeEventListener(type: 'abort', callback: () => void): void
}
export interface TransportRequest {
  path: string
  method: string
  headers: Record<string, string>
  body?: string | ArrayBuffer
  signal?: Cancellation
  binary?: boolean
}
export interface TransportResponse {
  status: number
  headers: Record<string, string>
  body: unknown
}
export type Transport = (request: TransportRequest) => Promise<TransportResponse>
export function assertApiPath(path: string) {
  const pathname=path.split('?')[0]
  if (
    !path.startsWith('/') ||
    path.startsWith('//') ||
    /[\\\u0000-\u0020#]/.test(path) ||
    /(?:^|\/)\.{1,2}(?:\/|$)/.test(pathname) ||
    /%2e|%2f|%5c/i.test(pathname)
  )
    throw new Error('API path must be same-origin')
}
export function createClient(transport: Transport) {
  let csrf = ''
  async function exchange(
    path: string,
    method: string,
    body?: unknown,
    signal?: Cancellation,
    extra?: Record<string, string>,
    binary = false,
  ) {
    assertApiPath(path)
    const bytes = body instanceof ArrayBuffer
    const headers = {
      Accept: binary
        ? 'application/octet-stream'
        : path.startsWith('/api/')
          ? 'application/vnd.api+json'
          : 'application/json',
      ...(bytes
        ? { 'Content-Type': 'application/octet-stream' }
        : {
            'Content-Type': path.startsWith('/api/')
              ? 'application/vnd.api+json'
              : 'application/json',
          }),
      ...(csrf ? { 'X-CSRF-Token': csrf } : {}),
      ...extra,
    }
    let response: TransportResponse
    try {
      if (signal?.aborted) throw new ApiError(0, 'cancelled', '请求已取消')
      response = await transport({
        path,
        method,
        headers,
        body:
          body === undefined
            ? undefined
            : bytes
              ? body
              : JSON.stringify(path.startsWith('/api/') ? writeDocument(path, body) : body),
        signal,
        binary,
      })
    } catch (error) {
      if (signal?.aborted) throw new ApiError(0, 'cancelled', '请求已取消')
      if (error instanceof ApiError) throw error
      throw new ApiError(0, 'network_error', '网络中断，请刷新记录核对操作结果')
    }
    if (signal?.aborted) throw new ApiError(0, 'cancelled', '请求已取消')
    if (response.status < 200 || response.status >= 300) {
      const detail = failureDetails(response.body)
      throw new ApiError(response.status, detail.code, detail.message, detail.meta)
    }
    return response
  }
  async function requestJson<T>(
    path: string,
    method = 'GET',
    body?: unknown,
    signal?: Cancellation,
    extra?: Record<string, string>,
  ): Promise<T> {
    const response = await exchange(path, method, body, signal, extra)
    const contentType = Object.entries(response.headers).find(
      ([k]) => k.toLowerCase() === 'content-type',
    )?.[1]
    if (path.startsWith('/api/') && contentType?.split(';')[0] !== 'application/vnd.api+json')
      return invalidDocument()
    return response.body as T
  }
  async function requestDocument(
    path: string,
    method = 'GET',
    body?: unknown,
    signal?: Cancellation,
    headers?: Record<string, string>,
  ): Promise<ResourceDocument> {
    const value = await requestJson<unknown>(path, method, body, signal, headers)
    if (
      !object(value) ||
      !('data' in value) ||
      'errors' in value ||
      (value.meta !== undefined && !object(value.meta))
    )
      return invalidDocument()
    if (Array.isArray(value.data)) value.data.forEach(decodeResource)
    else if (value.data !== null) decodeResource(value.data)
    const page = object(value.meta) ? value.meta.page : undefined
    if (
      page !== undefined &&
      (!object(page) ||
        !Number.isInteger(page.number) ||
        Number(page.number) < 1 ||
        !Number.isInteger(page.size) ||
        Number(page.size) < 1 ||
        typeof page.has_more !== 'boolean')
    )
      return invalidDocument()
    return value as unknown as ResourceDocument
  }
  async function requestResource<T>(
    path: string,
    method = 'GET',
    body?: unknown,
    signal?: Cancellation,
    headers?: Record<string, string>,
  ): Promise<T> {
    const doc = await requestDocument(path, method, body, signal, headers)
    return (
      Array.isArray(doc.data)
        ? doc.data.map(decodeResource)
        : doc.data === null
          ? null
          : decodeResource(doc.data)
    ) as T
  }
  async function requestCollection<T>(path: string, signal?: Cancellation): Promise<Collection<T>> {
    const doc = await requestDocument(path, 'GET', undefined, signal)
    if (!Array.isArray(doc.data)) return invalidDocument()
    return { items: doc.data.map(decodeResource) as T[], page: doc.meta?.page }
  }
  return {
    requestJson,
    requestDocument,
    requestResource,
    requestCollection,
    setCsrf(value: string) {
      csrf = value
    },
    async requestBinary(path: string, signal?: Cancellation) {
      const response = await exchange(path, 'GET', undefined, signal, undefined, true)
      if (!(response.body instanceof ArrayBuffer)) return invalidDocument()
      return response.body
    },
  }
}
export type Client = ReturnType<typeof createClient>
