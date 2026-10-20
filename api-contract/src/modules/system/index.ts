import { getJson } from '../../http'

export interface SystemDocument {
  data: {
    application: 'wekeeper'
    version: string
    stage: 'business'
  }
}

export function getSystemInfo(signal?: AbortSignal): Promise<SystemDocument> {
  return getJson('/api/v1/system', signal)
}
