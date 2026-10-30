import { nanoid } from 'nanoid/non-secure'

/** Request deduplication only; works without Web Crypto. Never use for secrets. */
export function createIdempotencyKey(): string {
  return nanoid(32)
}
