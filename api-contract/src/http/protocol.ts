/** Shared JSON:API transport, resource decoding and error handling. */
export interface ApiFailure {
  errors: Array<{
    status?: string
    code?: string
    title?: string
    detail?: string
    meta?: Record<string, unknown>
  }>
}

export function failureDetails(value: unknown): {
  code: string
  message: string
  meta?: Record<string, unknown>
} {
  if (
    typeof value === 'object' &&
    value !== null &&
    'errors' in value &&
    Array.isArray(value.errors)
  ) {
    const error: unknown = value.errors[0]
    if (typeof error === 'object' && error !== null) {
      return {
        meta:
          'meta' in error && typeof error.meta === 'object' && error.meta !== null
            ? (error.meta as Record<string, unknown>)
            : undefined,
        code: 'code' in error && typeof error.code === 'string' ? error.code : 'http_error',
        message:
          'detail' in error && typeof error.detail === 'string'
            ? error.detail
            : 'title' in error && typeof error.title === 'string'
              ? error.title
              : '服务暂时不可用',
      }
    }
  }
  return { code: 'http_error', message: '服务暂时不可用' }
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    public readonly meta?: Record<string, unknown>,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

export interface ResourceObject {
  type: string
  id: string
  attributes: Record<string, unknown>
}
export interface PageMeta {
  number: number
  size: number
  has_more: boolean
}
export interface Collection<T> {
  items: T[]
  page?: PageMeta
}
export interface ResourceDocument {
  data: ResourceObject | ResourceObject[] | null
  meta?: { page?: PageMeta; [key: string]: unknown }
}
export function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
export function invalidDocument(): never {
  throw new ApiError(200, 'invalid_document', '服务返回了不合法的 JSON:API 文档')
}
export function decodeResource(value: unknown): Record<string, unknown> {
  if (
    !object(value) ||
    typeof value.type !== 'string' ||
    !value.type ||
    typeof value.id !== 'string' ||
    !value.id ||
    !object(value.attributes) ||
    'id' in value.attributes ||
    'type' in value.attributes
  )
    return invalidDocument()
  return { ...value.attributes, id: value.id }
}
/** Explicit aliases for action routes whose target resource differs from the URL leaf. */
export function resourceType(path: string): string {
  const parts = path.split('?')[0].split('/').filter(Boolean)
  if (parts[0] !== 'api' || parts[1] !== 'v1') throw new Error('Unknown resource path')
  if (
    parts[2] === 'platform' &&
    parts[3] === 'wecom-assets' &&
    parts[4] === 'owner-scope-decisions'
  )
    return 'wecom-owner-scope-decisions'
  if (parts[2] === 'platform' && parts[3] === 'wecom-assets' && parts[4] === 'sync-runs')
    return 'wecom-asset-sync-runs'
  if (parts[2] === 'schools' && parts[4] === 'groups' && parts[5] === 'from-directory')
    return 'wecom-group-bindings'
  if (parts[2] === 'platform' && parts[3] === 'execution-devices') {
    if (parts[5] === 'enrollments') return 'device-enrollments'
    if (parts[5] === 'account-bindings') return 'device-account-bindings'
    return 'execution-devices'
  }
  if (parts[2] === 'platform' && parts[3] === 'sending-accounts') return 'sending-accounts'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-targets') return 'mobile-delivery-targets'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-strategies') return parts[6] === 'runs' ? 'mobile-delivery-strategy-runs' : 'mobile-delivery-strategies'
  if (parts[2] === 'schools' && parts[4] === 'delivery-groups') return 'delivery-groups'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-settings')
    return 'mobile-delivery-settings'
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-tasks') {
    if (parts[6] === 'previews') return 'mobile-delivery-previews'
    if (parts[6] === 'items') return 'mobile-delivery-items'
    if (parts[6] === 'events') return 'mobile-delivery-events'
    return 'mobile-delivery-tasks'
  }
  if (parts[2] === 'schools' && parts[4] === 'mobile-delivery-items')
    return parts[6] === 'resend-drafts' ? 'mobile-delivery-tasks' : 'mobile-delivery-items'
  if (parts[2] === 'auth') return 'sessions'
  if (
    parts[2] === 'platform' &&
    parts[3] === 'wecom-directory' &&
    parts[4] === 'credential-rotations'
  )
    return 'wecom-credential-rotations'
  if (parts[2] === 'accounts' && parts[4] === 'wecom-binding') return 'wecom-bindings'
  if (parts[2] === 'profile' || parts[2] === 'accounts') return 'accounts'
  if (parts[2] === 'platform' && parts[3] === 'resources') return 'resources'
  if (parts[2] === 'schools') {
    const kind = parts[4] ?? 'schools'
    if (kind === 'members') return 'accounts'
    if (kind === 'posts' && parts[6] === 'comments') return 'comments'
    if (kind === 'templates' && parts[6] === 'versions') return 'template-versions'
    if (kind === 'delivery-drafts' && parts[6] === 'attempts') return 'delivery-attempts'
    if (
      [
        'broadcast-tasks',
        'schools',
        'posts',
        'comments',
        'cases',
        'resources',
        'groups',
        'templates',
        'template-versions',
        'delivery-drafts',
        'delivery-attempts',
      ].includes(kind)
    )
      return kind
  }
  throw new Error('Unknown write resource path')
}
export function writeDocument(path: string, body: unknown) {
  if (!object(body)) throw new Error('Resource attributes must be an object')
  const { id, ...attributes } = body
  if ('type' in attributes || (id !== undefined && (typeof id !== 'string' || !id)))
    throw new Error('Invalid resource identity')
  return {
    data: {
      type: resourceType(path),
      ...(id === undefined ? {} : { id }),
      attributes,
    },
  }
}

export function decodeData<T>(document: ResourceDocument): T {
  return (
    Array.isArray(document.data)
      ? document.data.map(decodeResource)
      : document.data === null
        ? null
        : decodeResource(document.data)
  ) as T
}
