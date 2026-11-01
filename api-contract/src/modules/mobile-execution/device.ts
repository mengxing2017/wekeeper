import type { FlowValidation, FlowValidationResultInput } from './flows'
/** Device-only v7 capability agreement. Historical v6 results use an explicit evidence-only route. */
import { ApiError, decodeData, type ResourceDocument } from '../../http/client'
import type { DeviceEnvironment, CapabilityOpening, DeviceCapabilityContext, DeviceCapabilityConsent, DeviceCapabilityDecision } from './capabilities'
export function createMobileExecutionDeviceClient(options: { baseUrl: string; token: string; fetch?: typeof fetch }) {
  const base = new URL(options.baseUrl)
  if (base.protocol !== 'https:' || base.username || base.password || base.search || base.hash || base.pathname !== '/') {
    throw new Error('Device baseUrl must be a fixed HTTPS origin')
  }
  if (!options.token.startsWith('wk_device_') || options.token.length > 256 || /[\r\n]/u.test(options.token)) {
    throw new Error('A device credential is required')
  }
  const transport = options.fetch ?? fetch
  async function one<T>(path: string, type: string, attributes?: unknown, key?: string, version: 6 | 7 = 7): Promise<T> {
    if (attributes !== undefined && (!key?.trim() || [...key].length > 128)) throw new Error('A stable Idempotency-Key is required')
    const response = await transport(`${base.origin}/api/v${version}/device-execution/${path}`, {
      method: attributes === undefined ? 'GET' : 'POST', credentials: 'omit', redirect: 'error', cache: 'no-store',
      headers: { Accept: 'application/vnd.api+json', 'Content-Type': 'application/vnd.api+json',
        Authorization: `Bearer ${options.token}`, ...(key ? { 'Idempotency-Key': key } : {}) },
      body: attributes === undefined ? undefined : JSON.stringify({ data: { type, attributes } }),
    })
    const doc = await response.json() as ResourceDocument & { errors?: Array<{ code?: string; detail?: string }> }
    if (!response.ok) throw new ApiError(response.status, doc.errors?.[0]?.code ?? 'http_error', doc.errors?.[0]?.detail ?? 'Device request failed')
    if (response.headers.get('Content-Type')?.split(';')[0] !== 'application/vnd.api+json' || !doc.data || (Array.isArray(doc.data) || doc.data.type !== type)) {
      throw new ApiError(response.status, 'invalid_document', 'Invalid device JSON:API document')
    }
    return decodeData<T>(doc)
  }
  return {
    openingContext: () => one<{id: string; progress: CapabilityOpening}>('opening-context', 'mobile-device-opening-contexts'),
    reportEnvironment: (input: DeviceEnvironment['snapshot'], key: string) => one<DeviceEnvironment>('environment-reports', 'mobile-device-environments', input, key),
    capabilityContext: async () => {
      const context=await one<DeviceCapabilityContext>('capability-context', 'mobile-device-capability-contexts')
      if(context.protocol_version!==7 || !Array.isArray(context.scopes) || context.scopes.some(item =>
        !item?.scope || item.scope.schema!=='shared_mobile_execution_scope/1' || 'school_id' in item.scope || 'grant_id' in item.scope)) {
        throw new ApiError(200,'invalid_document','Invalid shared device capability scope')
      }
      return context
    },
    decideCapability: (input: DeviceCapabilityDecision, key: string) => one<DeviceCapabilityConsent>('capability-consents', 'mobile-device-capability-consents', input, key),
    // Evidence recovery only; no v6 inbox, school consent or launch operation remains exposed.
    validationResult: (id: string, input: FlowValidationResultInput, key: string) => one<FlowValidation>(`flow-validations/${encodeURIComponent(id)}/results`, 'mobile-execution-flow-validations', input, key, 6),
  }
}
