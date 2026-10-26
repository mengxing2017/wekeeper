import type { Client, Cancellation } from '../../http/client'
import { schoolPath } from '../schools'
export interface UploadedMedia {
  id: string
  url: string
}
export function createMediaClient(client: Client) {
  return {
    upload: (school: string, bytes: ArrayBuffer) =>
      client.requestResource<UploadedMedia>(`${schoolPath(school)}/media`, 'POST', bytes),
    download(school: string, path: string, signal?: Cancellation) {
      const prefix = `${schoolPath(school)}/media/`
      if (!path.startsWith(prefix) || !/^[0-9a-f-]{36}$/i.test(path.slice(prefix.length)))
        throw new Error('图片必须属于当前学校')
      return client.requestBinary(path, signal)
    },
  }
}
