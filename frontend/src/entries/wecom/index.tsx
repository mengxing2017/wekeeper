import { Link } from 'react-router-dom'
import { ShareWorkspace } from '../../features/operations/share-workspace'

export default function WecomEntry() {
  return <main className="wecom-shell">
    <header className="wecom-header"><Link className="brand" to="/admin"><span className="brand-mark">W</span>WeKeeper</Link><span>企业微信</span></header>
    <ShareWorkspace />
  </main>
}
