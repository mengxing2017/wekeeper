import { ConfigProvider, Layout, Typography } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import { Link } from 'react-router-dom'
import { Workspace } from '../../features/admin/workspace'

export default function AdminEntry() {
  return <ConfigProvider locale={zhCN} theme={{ token: { colorPrimary: '#25624b', borderRadius: 10 } }}>
    <Layout className="admin-layout">
      <Layout.Header className="site-header">
        <Link className="brand" to="/admin">WeKeeper<span>校园内容运营</span></Link>
        <Link className="header-link" to="/wecom">企业微信入口 ↗</Link>
      </Layout.Header>
      <Layout.Content className="admin-content">
        <Typography.Text className="eyebrow">WORKSPACE / 工作台</Typography.Text>
        <Typography.Title>运营工作台</Typography.Title>
        <Typography.Paragraph type="secondary">从内容审核到群内人工分享，在同一个工作空间完成校园内容运营。</Typography.Paragraph>
        <Workspace />
      </Layout.Content>
      <Layout.Footer className="site-footer">WeKeeper · 校园社区与人工分享</Layout.Footer>
    </Layout>
  </ConfigProvider>
}
