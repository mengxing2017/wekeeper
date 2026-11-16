import {afterEach, expect, it, vi} from 'vitest'
import {createScopedMaterialClient, createAdPlacementClient} from '../src/modules/advertising/material-client'
import {createBusinessDirectoryClient} from '../src/modules/business-directory/client'
import {createSourceReceiptsClient} from '../src/modules/source-access/receipts-client'
import {createMessageClient} from '../src/modules/source-access/automation-client'
import {sourceAccess} from '../src/modules/source-access'

afterEach(() => vi.unstubAllGlobals())

it('uses the same encoded pagination contract for all admin list clients', async () => {
  const requestResource = vi.fn().mockResolvedValue({items: [], has_more: false})
  const transport = {requestResource}
  const materials = createScopedMaterialClient(transport)
  const placements = createAdPlacementClient(transport)
  const directory = createBusinessDirectoryClient(transport)
  const receipts = createSourceReceiptsClient(transport)
  const messages = createMessageClient(transport)
  for (const [call, path] of [
    [() => materials.list(2, 1), '/api/v2/admin/ad-materials'],
    [() => materials.versions('a/b', 2, 1), '/api/v2/admin/ad-materials/a%2Fb/versions'],
    [() => placements.list(2, 1), '/api/v2/admin/ad-placements'],
    [() => directory.platforms(2, 1), '/api/v2/admin/business-platforms'],
    [() => directory.units('a/b', 2, 1), '/api/v2/admin/business-platforms/a%2Fb/units'],
    [() => receipts.list('a/b', 2, 1), '/api/v2/admin/source-routes/a%2Fb/receipts'],
    [() => messages.list('', '', 2, 1), '/api/v2/admin/messages'],
  ] as const) {
    await call()
    expect(requestResource).toHaveBeenLastCalledWith(`${path}?page%5Bnumber%5D=2&page%5Bsize%5D=1`)
  }
  await messages.list()
  expect(requestResource).toHaveBeenLastCalledWith('/api/v2/admin/messages?page%5Bnumber%5D=1&page%5Bsize%5D=30')
})

it('preserves source filters while encoding the new pagination fields', async () => {
  const fetchMock = vi.fn().mockImplementation(async () => Response.json(
    {data: {type: 'lists', id: 'list', attributes: {items: [], has_more: false}}},
    {headers: {'content-type': 'application/vnd.api+json'}},
  ))
  vi.stubGlobal('fetch', fetchMock)
  for (const kind of ['source-credentials', 'source-routes'] as const) {
    expect(await sourceAccess.list(kind, 2, '群 & 名', 'active', 'a/b', '2026-11-16T00:00:00Z', 1)).toEqual({id: 'list', items: [], has_more: false})
    const url = new URL(fetchMock.mock.lastCall![0], 'https://example.org')
    expect(url.pathname).toBe(`/api/v2/admin/${kind}`)
    expect(Object.fromEntries(url.searchParams)).toEqual({
      'page[number]': '2', 'page[size]': '1', search: '群 & 名', status: 'active',
      key_id: 'a/b', valid_until: '2026-11-16T00:00:00Z',
    })
  }
  await sourceAccess.routes('key-one', 2, 1)
  expect(fetchMock.mock.lastCall![0]).toBe('/api/v2/admin/source-credentials/key-one/routes?page%5Bnumber%5D=2&page%5Bsize%5D=1')
})
