export function taskError(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : ''
  const messages: Record<string, string> = {
    forbidden: '当前学校权限已变化。请重新选择有权限的学校；人工确认需要本校管理员权限。',
    unauthorized: '登录已失效，请重新登录后查看任务记录，核实原操作结果。',
  }
  return messages[code] || (error instanceof Error ? error.message : '操作失败，请刷新后重试')
}
