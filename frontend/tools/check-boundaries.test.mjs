import { test } from 'node:test'
import assert from 'node:assert/strict'
import { violations } from './check-boundaries.mjs'
test('checks static, type-only, re-export and dynamic imports, without client boundary bypasses', () => {
  for (const statement of [
    "import type {User} from '@wekeeper/api-contract/identity'",
    "export * from '@wekeeper/api-contract/business'",
    "const x=import('@wekeeper/api-contract/client')",
  ]) {
    assert.ok(
      violations(
        'apps/web/src/pages/index.tsx',
        statement,
      ).length,
    )
  }
  assert.equal(
    violations(
      'apps/web/src/features/identity/api/index.ts',
      "export type {User} from '@wekeeper/api-contract/identity'",
    ).length,
    0,
  )
  assert.ok(
    violations('apps/web/src/main.ts', "import x from '../../../../api-contract/src/http'").length,
  )
  assert.ok(violations('apps/web/src/main.ts', "import x from '../../miniapp/src/main'").length)
  assert.ok(violations('packages/core/index.ts', "export * from '../../apps/web/src/main'").length)
})

test('feature API imports use the contract package exports', () => {
  const file = 'apps/web/src/features/identity/api/index.ts'
  for (const spec of ['@wekeeper/api-contract', '@wekeeper/api-contract/unsupported', '@wekeeper/api-contract/src/http']) {
    assert.deepEqual(violations(file, `export * from '${spec}'`), ['只能引用 contract 声明的包导出'])
  }
  assert.deepEqual(violations(file, "export * from '@wekeeper/api-contract/mobile-execution/shared-send/v2'"), [])
})
