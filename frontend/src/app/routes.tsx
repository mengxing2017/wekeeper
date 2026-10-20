import { lazy, Suspense } from 'react'
import { Link, Navigate, Route, Routes } from 'react-router-dom'

const CommunityEntry = lazy(() => import('../features/community'))
const AdminEntry = lazy(() => import('../entries/admin'))
const WecomEntry = lazy(() => import('../entries/wecom'))

export function AppRoutes() {
  return <Suspense fallback={<main className="loading" role="status">正在加载…</main>}>
    <Routes>
      <Route path="/" element={<Navigate to="/admin" replace />} />
      <Route path="/admin" element={<AdminEntry />} />
      <Route path="/community" element={<CommunityEntry />} />
      <Route path="/community/:school/posts/:id" element={<CommunityEntry />} />
      <Route path="/wecom" element={<WecomEntry />} />
      <Route path="*" element={<main className="not-found"><h1>页面不存在</h1><Link to="/admin">返回运营入口</Link></main>} />
    </Routes>
  </Suspense>
}
