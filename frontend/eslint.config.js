import js from '@eslint/js'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['src/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': ['error', {
        patterns: [{ group: ['@wekeeper/api-contract', '@wekeeper/api-contract/*'], message: '只在 feature/api/index.ts 引用接口包。' }],
      }],
      'no-restricted-globals': ['error', { name: 'fetch', message: '通过 feature/api 与共享接口包调用 API。' }],
    },
  },
  { files: ['src/features/**/api/index.ts'], rules: { 'no-restricted-imports': 'off' } },
)
