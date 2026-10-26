/** Browser compatibility facade. New platforms inject their own transport. */
import { createClient, type Cancellation, type Transport } from './client'
export * from './client'

export const browserTransport: Transport = async (request) => {
  const controller = new AbortController()
  const cancel = () => controller.abort()
  if (request.signal?.aborted) cancel()
  request.signal?.addEventListener('abort', cancel, { once: true })
  try {
    const response = await fetch(request.path, {
      method: request.method,
      credentials: 'same-origin',
      headers: request.headers,
      body: request.body,
      signal: controller.signal,
    })
    let body: unknown
    try {
      body = request.binary && response.ok ? await response.arrayBuffer() : await response.json()
    } catch {
      body = undefined
    }
    return {
      status: response.status,
      headers: Object.fromEntries(response.headers.entries()),
      body,
    }
  } finally {
    request.signal?.removeEventListener('abort', cancel)
  }
}
const client = createClient(browserTransport)
export const setCsrf = client.setCsrf
export const requestDocument = client.requestDocument
export const requestCollection = client.requestCollection
// Legacy Web clients accept Blob; the protocol and other platforms only see bytes.
export async function requestResource<T>(
  path: string,
  method = 'GET',
  body?: unknown,
  signal?: Cancellation,
  headers?: Record<string, string>,
): Promise<T> {
  return client.requestResource(
    path,
    method,
    typeof Blob !== 'undefined' && body instanceof Blob ? await body.arrayBuffer() : body,
    signal,
    headers,
  )
}
export const requestJson = client.requestJson
export function getJson<T>(path: string, signal?: Cancellation): Promise<T> {
  return client.requestJson(path, 'GET', undefined, signal)
}
