import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClientProvider } from '@tanstack/react-query'
import { AppRoutes } from './app/routes'
import { queryClient } from './app/query-client'
import './shared/styles.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter><AppRoutes /></BrowserRouter>
    </QueryClientProvider>
  </React.StrictMode>,
)
