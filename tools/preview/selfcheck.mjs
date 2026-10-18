import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import vm from 'node:vm'

const app = readFileSync(new URL('../../renderer/app.js', import.meta.url), 'utf8')
function between (start, end) {
  const from = app.indexOf(start), to = app.indexOf(end, from)
  assert(from >= 0 && to > from, `Could not find ${start}`)
  return app.slice(from, to).trim()
}

const sales = app.match(/const SALES = \[[\s\S]*?\n\]/)?.[0]
const nextSale = app.match(/const next = (SALES\.find\(s => new Date\(s.start\) > now\))/)?.[1]
assert(sales && nextSale, 'Could not find the sale data or next-sale expression')
const label = { textContent: '' }
const context = vm.createContext({ document: { getElementById: () => label } })
vm.runInContext([
  sales,
  between('const DEFAULT_SETTINGS = {', '\nconst UI_PREFS_KEY'),
  between('const UI_PREFS_KEY', '\nfunction readUiPrefs'),
  'const initialUiPrefs = mergeUiPrefs(null)',
  between('const state = {', '\nlet shortcutsDialog'),
  between('function filteredSorted (', '\nfunction syncMinDiscount'),
  between('function storeKeyFor (', '\nconst gameStoreName'),
  between('function escHtml (', '\n// ─── Boot'),
  between('function updateRefreshAge (', '\nfunction updateLastCheckedLabels'),
  'globalThis.helpers = { SALES, state, filteredSorted, storeKeyFor, escHtml, updateRefreshAge, mergeUiPrefs, dealShops }',
].join('\n'), context)

const h = context.helpers
assert.equal(h.escHtml(`<tag a="&">'`), '&lt;tag a=&quot;&amp;&quot;&gt;\'')
assert.equal(h.storeKeyFor('Steam'), 'steam')
assert.equal(h.storeKeyFor('Epic Games'), 'epic')
assert.equal(h.storeKeyFor('GOG Galaxy'), 'gog')

const merged = h.mergeUiPrefs({
  version: 1, page: 'wishlist',
  lists: { deals: { sort: 'price', minDiscount: 75, shop: 'GOG', query: 'not saved' }, storesales: { sort: 'name', minDiscount: 50 }, wishlist: { sort: 'name', onSaleOnly: true } },
})
assert.equal(merged.page, 'wishlist')
assert.equal(merged.lists.deals.sort, 'price')
assert.equal(merged.lists.deals.minDiscount, 75)
assert.equal(merged.lists.deals.shop, 'GOG')
assert.equal(h.mergeUiPrefs(null).lists.deals.shop, '')
for (const shop of ['', 'x'.repeat(65), 42, null]) assert.equal(h.mergeUiPrefs({ version: 1, lists: { deals: { shop } } }).lists.deals.shop, '')
assert.equal(h.mergeUiPrefs({ version: 1, lists: { deals: { shop: 'x'.repeat(64) } } }).lists.deals.shop, 'x'.repeat(64))
assert.equal('shop' in merged.lists.storesales, false)
assert.equal(merged.lists.wishlist.onSaleOnly, true)
assert.equal('query' in merged.lists.deals, false)
assert.equal(h.mergeUiPrefs({ version: 2, page: 'settings' }).page, 'free')

const items = [
  { title: 'Beta B', price: 8, cut: 50, shop: 'Steam' }, { title: 'Alpha', price: 5, cut: 90, shop: 'GOG' },
  { title: 'Beta A', price: 3, cut: 10, shop: 'Steam' }, { title: 'Gamma', price: 2, cut: 0, shop: 'Fanatical' },
]
const filters = h.state.listFilters.deals
filters.query = 'BETA'; filters.sort = 'name'
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Beta A","Beta B"]')
filters.query = ''; filters.sort = 'price'
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Gamma","Beta A","Alpha","Beta B"]')
filters.sort = 'discount'; filters.minDiscount = 75
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Alpha"]')
filters.minDiscount = 0; filters.onSaleOnly = true
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Alpha","Beta B","Beta A"]')
filters.onSaleOnly = false; filters.shop = 'Steam'
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Beta B","Beta A"]')
filters.query = 'alpha'
assert.equal(h.filteredSorted(items, 'deals').length, 0)
filters.query = ''; filters.shop = ''
assert.equal(h.filteredSorted(items, 'deals').length, 4)
assert.equal(JSON.stringify(h.dealShops([...items, { title: 'No shop' }])), '[["Fanatical",1],["GOG",1],["Steam",2]]')

context.now = new Date('2026-10-18T12:00:00Z')
assert.equal(vm.runInContext(nextSale, context).name, 'Winter Sale') // Steam Scream is a fest, not listed in SALES
h.state.lastChecked = Date.now() - 7 * 60_000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated 7 min ago')
h.state.lastChecked = Date.now()
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated just now')

console.log('Renderer self-check passed (escaping, filters, store filter, UI preference merge, next sale, refresh age).')
