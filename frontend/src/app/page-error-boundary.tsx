import { Component, type ReactNode } from 'react'

export class PageErrorBoundary extends Component<{children: ReactNode}, {failed: boolean}> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    if (this.state.failed) return <main className="not-found" role="alert"><h1>页面暂时无法加载</h1><p>请重新加载页面；若仍失败，请检查前端服务与浏览器控制台。</p><button onClick={() => location.reload()}>重新加载</button> <a href="/admin">返回运营后台</a></main>
    return this.props.children
  }
}
