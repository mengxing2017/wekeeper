import type { Client, Cancellation } from '../../http/client'
const mediaPath = '/api/v1/wecom-distribution/media'
export interface UploadedMedia {
  id: string
  url: string
}
export function createMediaClient(client: Client) {
  return {
    upload: (bytes: ArrayBuffer) =>
      client.requestResource<UploadedMedia>(`${mediaPath}`, 'POST', bytes),
    download(path: string, signal?: Cancellation) {
      const prefix = `${mediaPath}/`
      if (!path.startsWith(prefix) || !/^[0-9a-f-]{36}$/i.test(path.slice(prefix.length)))
        throw new Error('图片地址无效')
      return client.requestBinary(path, signal)
    },
  }
}
