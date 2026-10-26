import { test } from 'node:test'
import assert from 'node:assert/strict'
import { violations } from './check-boundaries.mjs'
test('checks static, type-only, re-export and dynamic imports, including Vue scripts', () => {
  for (const statement of [
    "import type {User} from '@wekeeper/api-contract/identity'",
    "export * from '@wekeeper/api-contract/community'",
    "const x=import('@wekeeper/api-contract/client')",
  ]) {
    assert.ok(
      violations(
        'apps/miniapp/src/pages/index.vue',
        `<script setup lang="ts">${statement}</script>`,
      ).length,
    )
  }
  assert.equal(
    violations(
      'apps/miniapp/src/features/identity/api/index.ts',
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
