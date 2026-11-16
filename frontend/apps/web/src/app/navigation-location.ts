/** Historical addresses resolve to one visible business page. */
export function canonicalNavigation(params: URLSearchParams): URLSearchParams | null {
  const next = new URLSearchParams(params)
  if (params.get('view') === 'advertisement-rotations') {
    next.set('view', 'advertisements')
  } else if (params.get('view') === 'execution-devices' && params.get('subview') === 'usage') {
    next.set('view', 'shared-mobile-resources')
    next.delete('subview')
  } else {
    return null
  }
  return next
}
