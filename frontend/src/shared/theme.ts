import type { ThemeConfig } from 'antd'

// Keep the palette and typography aligned with tokens.css and huavd-ops.
export const appTheme: ThemeConfig = {
  token: {
    colorPrimary: '#08786c',
    colorInfo: '#08786c',
    colorSuccess: '#08786c',
    borderRadius: 5,
    fontSize: 14,
    fontFamily: 'Inter, "Noto Sans SC", sans-serif',
  },
  components: {
    Menu: {
      darkItemBg: '#17252b',
      darkItemColor: '#dce4e7',
      darkItemHoverBg: '#24353c',
      darkItemHoverColor: '#fff',
      darkItemSelectedBg: '#116b62',
      darkItemSelectedColor: '#fff',
    },
    Table: {
      cellFontSize: 14,
      cellPaddingBlock: 13,
      cellPaddingInline: 14,
      headerBg: '#f6f8f9',
      headerColor: '#53616e',
      borderColor: '#e4e8ec',
    },
  },
}
