import {DEFAULT_ADMIN_VIEW} from './navigation'
/** Historical addresses resolve to one visible business page. */
export function canonicalNavigation(params: URLSearchParams): URLSearchParams | null {
  const next = new URLSearchParams(params)
  if (!params.has('view')) {
    next.set('view', DEFAULT_ADMIN_VIEW)
  } else if (params.get('view') === 'advertisement-rotations') {
    next.set('view', 'advertisements')
  } else {
    return null
  }
  return next
}
