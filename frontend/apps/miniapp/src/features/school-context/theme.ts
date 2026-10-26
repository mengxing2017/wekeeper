import { computed } from 'vue'
import { schoolContext } from './context'
/** Keep school-provided values out of arbitrary inline CSS. */
export const schoolTheme = computed(() => {
  const value = schoolContext.schools.find((s) => s.id === schoolContext.id)?.config.theme_color
  const primary =
    typeof value === 'string' && /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i.test(value) ? value : '#5B8FF9'
  return `--primary:${primary};--secondary:#48C5BD`
})
