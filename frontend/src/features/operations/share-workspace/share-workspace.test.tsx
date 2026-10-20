import { afterEach, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ShareWorkspace } from './index'

afterEach(cleanup)

it('does not offer a send action before identity and group authorization exist', () => {
  render(<ShareWorkspace />)
  expect(screen.getByText('分享功能尚未开放')).toBeTruthy()
  expect(screen.queryByRole('button')).toBeNull()
})
