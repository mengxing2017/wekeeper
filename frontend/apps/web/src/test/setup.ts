import { beforeEach, vi } from 'vitest'

// JSDOM does not run CSS animations. Use Ant Design's supported motion setting
// so dialogs finish closing and loading icons do not wait for animation events.
vi.mock('@testing-library/react', async importOriginal => {
  const testingLibrary = await importOriginal<typeof import('@testing-library/react')>()
  const { ConfigProvider } = await import('antd')
  const { createElement } = await import('react')
  return {
    ...testingLibrary,
    render: (ui: Parameters<typeof testingLibrary.render>[0], options?: Parameters<typeof testingLibrary.render>[1]) => {
      const Wrapper = options?.wrapper
      return testingLibrary.render(ui, {
        ...options,
        wrapper: ({ children }) => createElement(ConfigProvider, { theme: { token: { motion: false } } },
          Wrapper ? createElement(Wrapper, null, children) : children),
      })
    },
  }
})

// JSDOM computes element styles, but does not implement pseudo-element styles.
// Keep real visibility checks while allowing Ant Design scrollbar measurements.
const getComputedStyle = typeof window === 'undefined' ? undefined : window.getComputedStyle.bind(window)

beforeEach(() => {
  if (!getComputedStyle) return
  vi.stubGlobal('getComputedStyle', (element: Element) => getComputedStyle(element))
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} })
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false, media: query, onchange: null,
    addListener: vi.fn(), removeListener: vi.fn(),
    addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
  }))
})
