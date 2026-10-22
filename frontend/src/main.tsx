import React from 'react'
import { ConfigProvider } from 'antd'
import { appTheme } from './shared/theme'
import zhCN from 'antd/locale/zh_CN'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClientProvider } from '@tanstack/react-query'
import { AppRoutes } from './app/routes'
import { queryClient } from './app/query-client'
import './shared/styles.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ConfigProvider locale={zhCN} theme={appTheme}>
        <BrowserRouter><AppRoutes /></BrowserRouter>
      </ConfigProvider>
    </QueryClientProvider>
  </React.StrictMode>,
)
