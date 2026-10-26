import { Link, useSearchParams } from 'react-router-dom'
export function ShareWorkspace() {
  const [params] = useSearchParams()
  const school = params.get('school')
  const query = new URLSearchParams({view:'broadcast-tasks'})
  if(school && /^[0-9a-f-]{36}$/i.test(school)) query.set('school',school)
  return <section className="page-content"><h1>分享入口已迁移</h1><p>请在运营后台创建企业微信群发任务，成员在企业微信确认执行。历史人工分享记录仍可核对。</p><Link to={`/admin?${query}`}>登录运营后台查看群发任务</Link></section>
}
