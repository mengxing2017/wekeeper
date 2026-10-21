import { requestResource } from '../../http'
export interface SystemInfo {
  id: string
  application: 'wekeeper'
  version: string
  stage: 'business'
}
export function getSystemInfo(signal?: AbortSignal): Promise<SystemInfo> {
  return requestResource('/api/v1/system', 'GET', undefined, signal)
}
