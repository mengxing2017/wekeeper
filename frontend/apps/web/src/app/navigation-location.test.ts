import {expect,it} from 'vitest'
import {canonicalNavigation} from './navigation-location'
it('normalizes only missing pages and preserves statistics filters and explicit deep links',()=>{
 expect(canonicalNavigation(new URLSearchParams('stats_tab=units&stats_grain=month'))?.toString()).toBe('stats_tab=units&stats_grain=month&view=statistics-overview')
 expect(canonicalNavigation(new URLSearchParams('view=broadcast-tasks&task=one'))).toBeNull()
 expect(canonicalNavigation(new URLSearchParams('view=missing'))).toBeNull()
})
