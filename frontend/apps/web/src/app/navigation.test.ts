// @vitest-environment node
import {expect,it} from 'vitest'
import {navigation,visibleNavigationKeys,canNavigate} from './navigation'
it('global administrators can open every current business page',()=>{
 expect(visibleNavigationKeys(true)).toEqual(navigation.map(n=>n.key))
 expect(new Set(navigation.map(n=>n.key)).size).toBe(navigation.length)
})
it('an account without the global administrator role has no business access',()=>{
 expect(visibleNavigationKeys(false)).toEqual([])
 for(const item of navigation) expect(canNavigate(item,false)).toBe(false)
})
