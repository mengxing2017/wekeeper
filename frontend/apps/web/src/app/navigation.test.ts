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

it('every visible entry identifies a distinct page or meaningful business mode',()=>{
 const pages=navigation.map(item=>JSON.stringify(item.page))
 expect(new Set(pages).size).toBe(pages.length)
})

it('keeps statistics as the explicit default and first independent entry',async()=>{
 const {DEFAULT_ADMIN_VIEW}=await import('./navigation')
 expect(DEFAULT_ADMIN_VIEW).toBe('statistics-overview')
 expect(navigation[0]).toMatchObject({key:DEFAULT_ADMIN_VIEW,topLevel:true,label:'统计概览'})
})
