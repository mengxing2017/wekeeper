import { fireEvent, waitFor } from '@testing-library/react'

export async function selectOption(control: HTMLElement, label: string) {
  fireEvent.mouseDown(control)
  fireEvent.click(control)
  await waitFor(() => {
    const option = [...document.querySelectorAll<HTMLElement>('.ant-select-dropdown:not(.ant-select-dropdown-hidden) .ant-select-item-option-content')]
      .find(node => node.textContent?.trim() === label)
    if (!option) throw new Error(`Option not found: ${label}`)
    fireEvent.click(option)
  })
}
