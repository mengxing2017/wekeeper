import { getInstanceByDom, init, use } from 'echarts/core'
import { BarChart, LineChart } from 'echarts/charts'
import {
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from 'echarts/components'
import { SVGRenderer } from 'echarts/renderers'
use([
  BarChart,
  LineChart,
  AriaComponent,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  SVGRenderer,
])
export { getInstanceByDom, init }
export type { EChartsType, EChartsCoreOption } from 'echarts/core'
