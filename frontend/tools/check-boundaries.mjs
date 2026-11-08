import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

export function imports(source, filename) {
  const chunks = filename.endsWith('.vue')
    ? [...source.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1])
    : [source]
  const found = []
  for (const chunk of chunks) {
    const tree = ts.createSourceFile(
      filename + '.tsx',
      chunk,
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TSX,
    )
    function visit(node) {
      if (
        (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
        node.moduleSpecifier &&
        ts.isStringLiteral(node.moduleSpecifier)
      )
        found.push(node.moduleSpecifier.text)
      if (
        ts.isCallExpression(node) &&
        (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
          (ts.isIdentifier(node.expression) && node.expression.text === 'require'))
      ) {
        if (node.arguments[0] && ts.isStringLiteralLike(node.arguments[0]))
          found.push(node.arguments[0].text)
        else found.push('<computed-import>')
      }
      if (
        ts.isImportTypeNode(node) &&
        ts.isLiteralTypeNode(node.argument) &&
        ts.isStringLiteral(node.argument.literal)
      )
        found.push(node.argument.literal.text)
      ts.forEachChild(node, visit)
    }
    visit(tree)
  }
  return found
}
export function violations(file, source) {
  const errors = []
  for (const spec of imports(source, file)) {
    if (spec === '<computed-import>') errors.push('动态导入必须使用可检查的字面量')
    if (spec.startsWith('@wekeeper/api-contract') && !/\/features\/.+\/api\/index\.ts$/.test(file))
      errors.push('只允许 feature/api/index.ts 引用 contract')
    const resolved = spec.startsWith('.')
      ? path.posix.normalize(path.posix.join(path.posix.dirname(file), spec))
      : spec
    if (spec.startsWith('.') && /(?:^|\/)api-contract(?:\/|$)/.test(resolved))
      errors.push('禁止通过源码相对路径绕过 contract 包导出')
    if (
      file.startsWith('packages/') &&
      (/apps\//.test(resolved) || /react|vue|antd|uni-app/.test(spec))
    )
      errors.push('纯共享包禁止依赖应用或 UI 框架')
    if (resolved.includes('apps/miniapp/') || /@wekeeper\/api-contract\/community(?:$|\/)/.test(spec))
      errors.push('论坛和小程序已迁至 linjian，禁止引用旧客户端')
    if (/(?:^|\/)uniapp\//.test(resolved) || resolved.includes('uniapp/config'))
      errors.push('禁止引用旧 uniapp 工程')
  }
  return errors
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
export function check() {
  const errors = []
  function walk(dir) {
    if (!fs.existsSync(dir)) return
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (['node_modules', 'dist'].includes(entry.name)) continue
      const file = path.join(dir, entry.name)
      if (entry.isDirectory()) walk(file)
      else if (/\.(?:ts|tsx|vue)$/.test(file)) {
        const relative = path.relative(root, file).split(path.sep).join('/')
        for (const error of violations(relative, fs.readFileSync(file, 'utf8')))
          errors.push(`${relative}: ${error}`)
      }
    }
  }
  walk(path.join(root, 'apps'))
  walk(path.join(root, 'packages'))
  if (errors.length) throw new Error(errors.join('\n'))
  console.log('Frontend import boundaries passed.')
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) check()
