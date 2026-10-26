import { mediaApi } from './api'
import { onScopeChange, scopeKey } from '../school-context/context'
const files = new Map<string, Promise<string>>()
const paths = new Set<string>()
let sequence = 0
function unlink(path: string) {
  uni.getFileSystemManager().unlink({ filePath: path, fail: () => {} })
}
onScopeChange(() => {
  files.clear()
  for (const path of paths) unlink(path)
  paths.clear()
})
export async function uploadImage(school: string, filePath: string) {
  const bytes = await new Promise<ArrayBuffer>((resolve, reject) =>
    uni.getFileSystemManager().readFile({
      filePath,
      success: (r) =>
        typeof r.data === 'string' ? reject(new Error('图片读取格式错误')) : resolve(r.data),
      fail: reject,
    }),
  )
  if (bytes.byteLength > 5 * 1024 * 1024) throw new Error('图片不能超过 5 MiB')
  return mediaApi.upload(school, bytes)
}
export function imagePath(school: string, source: string): Promise<string> {
  // Public external images carry no Authorization header.
  if (/^https:\/\//i.test(source)) return Promise.resolve(source)
  const key = `${scopeKey()}:${source}`
  const existing = files.get(key)
  if (existing) return existing
  const scope = scopeKey()
  const promise = (async () => {
    const bytes = await mediaApi.download(school, source)
    if (scope !== scopeKey()) throw new Error('学校或登录状态已改变')
    const path = `${(uni as typeof uni & { env: { USER_DATA_PATH: string } }).env.USER_DATA_PATH}/wekeeper-${Date.now()}-${sequence++}.img`
    await new Promise<void>((resolve, reject) =>
      uni
        .getFileSystemManager()
        .writeFile({ filePath: path, data: bytes, success: () => resolve(), fail: reject }),
    )
    if (scope !== scopeKey()) {
      unlink(path)
      throw new Error('学校或登录状态已改变')
    }
    paths.add(path)
    return path
  })().catch((error) => {
    files.delete(key)
    throw error
  })
  files.set(key, promise)
  return promise
}
