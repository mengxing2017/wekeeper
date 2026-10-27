import type { ThemeConfig } from 'antd'

// Ant Design Pro's light layout and default blue palette.
export const appTheme: ThemeConfig = {
  token: {
    colorPrimary: '#1677ff',
    colorInfo: '#1677ff',
    borderRadius: 6,
    fontSize: 14,
    fontFamily: 'var(--font-family-sans)',
    lineHeight: 22 / 14,
    controlHeight: 32,
    colorBgLayout: '#f5f5f5',
  },
  components: {
    Menu: {
      itemBorderRadius: 6,
      itemHeight: 40,
      itemSelectedBg: '#e6f4ff',
      itemSelectedColor: '#1677ff',
    },
    Card: { headerFontSize: 16, headerFontSizeSM: 14 },
    // ProTable uses middle density: 12px vertically and 16px horizontally.
    Table: { cellFontSize: 14, cellPaddingBlock: 12, cellPaddingInline: 16, headerBg: '#fafafa' },
  },
}
