export interface ApiFailure {
  error: { code: string; message: string }
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message)
    this.name = 'ApiError'
  }
}

// Same-origin only. Authentication/CSRF will be added with the identity module.
// No public mutation helper is exposed before those protections exist.
export async function getJson<T>(path: string, signal?: AbortSignal): Promise<T> {
  if (!path.startsWith('/') || path.startsWith('//') || /[\\\u0000-\u0020]/.test(path)) {
    throw new Error('API path must be same-origin')
  }
  let response: Response
  try {
    response = await fetch(path, {
      headers: { Accept: 'application/json' }, credentials: 'same-origin', signal,
    })
  } catch (error) {
    if (signal?.aborted) throw error
    throw new ApiError(0, 'network_error', '无法连接服务，请检查网络后重试')
  }
  if (!response.ok) {
    let failure: ApiFailure | undefined
    try { failure = await response.json() as ApiFailure } catch { /* proxy may return HTML */ }
    throw new ApiError(response.status, failure?.error?.code ?? 'http_error', failure?.error?.message ?? '服务暂时不可用')
  }
  return response.json() as Promise<T>
}
