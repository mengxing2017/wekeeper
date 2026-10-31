import type { DeviceFlowValidation, FlowValidation, FlowValidationResultInput } from './flows'
/** v6 device-only consent transport. No browser session or legacy business DTOs. */
import { ApiError, decodeData, type ResourceDocument } from '../../http/client'
export interface BusinessDeviceScope {
  schema: 'business_device_scope/3'; platform_source: 'wekeeper'; device_id: string;
  school_id: string; grant_id: string; grant_revision: number; purpose: string;
  app_key: 'wecom'; capability: 'group_text';
}
export interface DeviceConsent {
  id: string; device_id: string; school_id: string; grant_id: string; grant_revision: number;
  scope_digest: string; scope: BusinessDeviceScope; revision: number; accepted: boolean;
  credential_generation: number; updated_at: string;
}
export interface ConsentContext {
  id: string; protocol_version: 6;
  scopes: Array<{ id: string; school_name: string; scope: BusinessDeviceScope;
    scope_digest: string; consent: DeviceConsent | null; accepted: boolean }>;
}
export interface ConsentInput {
  grant_id: string; grant_revision: number; scope_digest: string;
  expected_revision: number; accepted: boolean;
}
export function createMobileExecutionDeviceClient(options: { baseUrl: string; token: string; fetch?: typeof fetch }) {
  const base = new URL(options.baseUrl)
  if (base.protocol !== 'https:' || base.username || base.password || base.search || base.hash || base.pathname !== '/') {
    throw new Error('Device baseUrl must be a fixed HTTPS origin')
  }
  if (!options.token.startsWith('wk_device_') || options.token.length > 256 || /[\r\n]/u.test(options.token)) {
    throw new Error('A device credential is required')
  }
  const transport = options.fetch ?? fetch
  async function one<T>(path: string, type: string, attributes?: unknown, key?: string, multiple = false): Promise<T> {
    if (attributes !== undefined && (!key?.trim() || [...key].length > 128)) throw new Error('A stable Idempotency-Key is required')
    const response = await transport(`${base.origin}/api/v6/device-execution/${path}`, {
      method: attributes === undefined ? 'GET' : 'POST', credentials: 'omit', redirect: 'error', cache: 'no-store',
      headers: { Accept: 'application/vnd.api+json', 'Content-Type': 'application/vnd.api+json',
        Authorization: `Bearer ${options.token}`, ...(key ? { 'Idempotency-Key': key } : {}) },
      body: attributes === undefined ? undefined : JSON.stringify({ data: { type, attributes } }),
    })
    const doc = await response.json() as ResourceDocument & { errors?: Array<{ code?: string; detail?: string }> }
    if (!response.ok) throw new ApiError(response.status, doc.errors?.[0]?.code ?? 'http_error', doc.errors?.[0]?.detail ?? 'Device request failed')
    if (response.headers.get('Content-Type')?.split(';')[0] !== 'application/vnd.api+json' || !doc.data || (multiple ? !Array.isArray(doc.data) || doc.data.some(r => r.type !== type) : Array.isArray(doc.data) || doc.data.type !== type)) {
      throw new ApiError(response.status, 'invalid_document', 'Invalid device JSON:API document')
    }
    return decodeData<T>(doc)
  }
  return {
    // Inbox: the first 30 pending jobs. Completed/expired jobs leave this queue.
    validations: () => one<DeviceFlowValidation[]>('flow-validations', 'mobile-execution-flow-validations', undefined, undefined, true),
    validationResult: (id: string, input: FlowValidationResultInput, key: string) => one<FlowValidation>(`flow-validations/${encodeURIComponent(id)}/results`, 'mobile-execution-flow-validations', input, key),
    authorizationContext: () => one<ConsentContext>('authorization-context', 'mobile-device-consent-contexts'),
    consent: (input: ConsentInput, key: string) => one<DeviceConsent>('consents', 'mobile-device-consents', input, key),
  }
}
