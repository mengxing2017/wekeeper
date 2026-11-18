/** Historical addresses resolve to one visible business page. */
export function canonicalNavigation(params: URLSearchParams): URLSearchParams | null {
  const next = new URLSearchParams(params)
  if (params.get('view') === 'advertisement-rotations') {
    next.set('view', 'advertisements')
  } else {
    return null
  }
  return next
}
