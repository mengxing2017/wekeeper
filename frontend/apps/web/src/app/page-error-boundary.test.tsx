import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { PageErrorBoundary } from './page-error-boundary'

afterEach(() => { cleanup(); vi.restoreAllMocks() })
it('renders recovery controls when a page fails to render', () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  function Broken(): never { throw new Error('chunk or render failure') }
  render(<PageErrorBoundary><Broken /></PageErrorBoundary>)
  expect(screen.getByRole('alert')).toBeTruthy()
  expect(screen.getByRole('button', { name: '重新加载' })).toBeTruthy()
})
