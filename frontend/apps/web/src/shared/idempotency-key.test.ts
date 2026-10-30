import { afterEach, expect, it, vi } from 'vitest'
import { createIdempotencyKey } from './idempotency-key'

const uuidV4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
afterEach(() => vi.unstubAllGlobals())

it('uses the native UUID method with its Crypto receiver', () => {
  const crypto = {
    randomUUID() {
      expect(this).toBe(crypto)
      return '12345678-1234-4123-8123-123456789abc'
    },
    getRandomValues: vi.fn(),
  }
  vi.stubGlobal('crypto', crypto)
  expect(createIdempotencyKey()).toBe('12345678-1234-4123-8123-123456789abc')
  expect(crypto.getRandomValues).not.toHaveBeenCalled()
})

it.each([
  [0x00, '00000000-0000-4000-8000-000000000000'],
  [0xff, 'ffffffff-ffff-4fff-bfff-ffffffffffff'],
])('sets v4 and variant bits when randomUUID is unavailable (byte=%s)', (byte, expected) => {
  const crypto = {
    getRandomValues(bytes: Uint8Array) {
      expect(this).toBe(crypto)
      expect(bytes.length).toBe(16)
      return bytes.fill(byte as number)
    },
  }
  vi.stubGlobal('crypto', crypto)
  expect(createIdempotencyKey()).toBe(expected)
})

it('generates distinct v4 request keys with getRandomValues alone', () => {
  const crypto = globalThis.crypto
  vi.stubGlobal('crypto', { getRandomValues: crypto.getRandomValues.bind(crypto) })
  const keys = Array.from({ length: 128 }, createIdempotencyKey)
  expect(keys.every((key) => uuidV4.test(key))).toBe(true)
  expect(new Set(keys).size).toBe(keys.length)
})

it.each([undefined, {}])('reports unavailable secure randomness instead of using weak keys (%s)', (crypto) => {
  vi.stubGlobal('crypto', crypto)
  expect(createIdempotencyKey).toThrow('当前浏览器不支持安全随机数')
})
