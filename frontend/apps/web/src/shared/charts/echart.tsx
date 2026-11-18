import { useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Button, theme } from 'antd'
import type { EChartsCoreOption, EChartsType } from './echarts'

export function EChart({
  option,
  label,
  height = 320,
  onSelect,
  zoom = false,
}: {
  option: EChartsCoreOption
  label: string
  height?: number
  zoom?: boolean
  onSelect?: (id: string, series: string) => void
}) {
  const host = useRef<HTMLDivElement>(null)
  const instance = useRef<EChartsType | undefined>(undefined)
  const release = useRef<() => void>(() => {})
  const [failed, setFailed] = useState(false)
  const { token } = theme.useToken()
  const themedOption = useMemo(
    () => ({ ...option, textStyle: { color: token.colorText, fontFamily: token.fontFamily } }),
    [option, token.colorText, token.fontFamily],
  )
  const latest = useRef({ option: themedOption, onSelect })
  useEffect(() => {
    latest.current = { option: themedOption, onSelect }
  })
  useEffect(() => {
    let cancelled = false
    let observer: ResizeObserver | undefined
    let chart: EChartsType | undefined
    const dispose = () => {
      observer?.disconnect()
      chart?.off('click')
      chart?.dispose()
      chart = undefined
      instance.current = undefined
    }
    release.current = dispose
    const fail = () => {
      if (!cancelled) setFailed(true)
      dispose()
    }
    void import('./echarts')
      .then(({ init }) => {
        if (cancelled || !host.current) return
        const render = () => {
          if (cancelled || !host.current || host.current.clientWidth === 0) return
          try {
            if (!chart) {
              chart = init(host.current, undefined, { renderer: 'svg' })
              instance.current = chart
              chart.on('click', (params) => {
                const point = params.data as { id?: unknown; raw?: unknown } | undefined
                if (
                  typeof point?.id === 'string' &&
                  point.raw !== '0' &&
                  typeof params.seriesId === 'string'
                ) {
                  latest.current.onSelect?.(point.id, params.seriesId)
                }
              })
              chart.setOption(latest.current.option)
            } else chart.resize()
          } catch {
            fail()
          }
        }
        observer = new ResizeObserver(render)
        observer.observe(host.current)
        render()
      })
      .catch(fail)
    return () => {
      cancelled = true
      dispose()
    }
  }, [])
  useEffect(() => {
    try {
      instance.current?.setOption(themedOption, { replaceMerge: ['series'] })
    } catch {
      setFailed(true)
      release.current()
    }
  }, [themedOption])
  return (
    <>
      {failed && <Alert type="warning" title="图表暂不可用，请使用数据表核对" />}
      <div
        data-chart="true"
        ref={host}
        role="img"
        aria-label={`${label}，精确数据及操作见数据表`}
        style={{ width: '100%', minWidth: 0, height, display: failed ? 'none' : undefined }}
      />
      {zoom && (
        <p>
          <Button
            size="small"
            onClick={() =>
              instance.current?.dispatchAction({ type: 'dataZoom', start: 0, end: 100 })
            }
          >
            重置缩放
          </Button>{' '}
          缩放仅影响图形，CSV 导出完整查询范围。
        </p>
      )}
    </>
  )
}
