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
  between('const CLAIMED_KEY', '\nconst state = {'),
  between('const state = {', '\nlet shortcutsDialog'),
  between('function normalizeTitle (', '\nfunction announceStatus'),
  between('function storeKeyFor (', '\nconst gameStoreName'),
  between('function escHtml (', '\n// ─── Boot'),
  between('function seenIds (', '\nconst unseen'),
  between('function updateRefreshAge (', '\nfunction updateLastCheckedLabels'),
  'globalThis.helpers = { SALES, state, filteredSorted, storeKeyFor, escHtml, updateRefreshAge, mergeUiPrefs, dealShops, parseClaimed, pruneClaimed, toggleClaimed, claimedLast, normalizeTitle, hiddenTitle, searchRank, parseSeen, newIds, mergeSeen, wishlistKey, seenIds }',
].join('\n'), context)

const h = context.helpers
assert.equal(JSON.stringify(h.state.settings.hidden), '[]')
assert.equal(h.escHtml(`<tag a="&">'`), '&lt;tag a=&quot;&amp;&quot;&gt;\'')
assert.equal(h.storeKeyFor('Steam'), 'steam')
assert.equal(h.storeKeyFor('Epic Games'), 'epic')
assert.equal(h.storeKeyFor('GOG Galaxy'), 'gog')
assert.equal(h.normalizeTitle('  The\t LONG  Way\nHome '), 'the long way home')
h.state.settings.hidden = ['hades']
assert.equal(h.hiddenTitle(' Hades '), true)
assert.equal(h.hiddenTitle('Control'), false)
assert.equal(h.hiddenTitle('hades ii', ['Hades  II']), true)

const games = ['Portal Knights', 'Super Portal', 'portal', 'Hades', 'The  PORTAL  2'].map(title => ({ title }))
assert.equal(JSON.stringify(h.searchRank(games, ' PORTAL ').map(g => g.title)), '["Portal Knights","portal","Super Portal","The  PORTAL  2"]')
assert.equal(JSON.stringify(h.searchRank(games, 'portal 2').map(g => g.title)), '["The  PORTAL  2"]')
assert.equal(h.searchRank(games, '  ').length, 0)
assert.equal(h.searchRank(games, 'zzz').length, 0)
assert.equal(h.searchRank(games, 'a', 2).length, 2)

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

assert.equal(JSON.stringify(h.state.claimed), '[]') // no localStorage in node: falls back to nothing claimed
assert.equal(JSON.stringify(h.parseClaimed('["a",2,"b"]')), '["a","b"]')
assert.equal(JSON.stringify(h.parseClaimed('{"a":1}')), '[]')
assert.equal(JSON.stringify(h.parseClaimed('not json')), '[]')
assert.equal(JSON.stringify(h.parseClaimed(null)), '[]')
assert.equal(JSON.stringify(h.toggleClaimed(['a'], 'b')), '["a","b"]')
assert.equal(JSON.stringify(h.toggleClaimed(['a', 'b'], 'a')), '["b"]')
assert.equal(JSON.stringify(h.pruneClaimed(['a', 'gone', 'b'], [{ id: 'b' }, { id: 'a' }, { id: 'c' }])), '["a","b"]')
assert.equal(JSON.stringify(h.claimedLast([{ id: 'a' }, { id: 'b' }, { id: 'c' }, { id: 'd' }], ['a', 'c']).map(g => g.id)), '["b","d","a","c"]')

assert.equal(JSON.stringify(h.state.seen), '{"deals":null,"storesales":null,"wishlist":null}') // no localStorage: first run, nothing tagged
assert.equal(JSON.stringify(h.parseSeen('{"deals":["a",2],"storesales":"x"}')), '{"deals":["a"],"storesales":null,"wishlist":null}')
assert.equal(JSON.stringify(h.parseSeen('not json')), '{"deals":null,"storesales":null,"wishlist":null}')
assert.equal(h.newIds(['a', 'b'], null).size, 0)
assert.equal(JSON.stringify([...h.newIds(['a', 'b', 'c'], ['b'])]), '["a","c"]')
assert.equal(JSON.stringify(h.mergeSeen({ deals: ['old'], storesales: null }, 'deals', ['a', 'b', 'a'])), '{"deals":["a","b"],"storesales":null}')
const seenBefore = { deals: ['a'], storesales: ['s'] }
assert.equal(h.mergeSeen(seenBefore, 'storesales', []), seenBefore) // empty list may be a failed load: keep the old set

// Wishlist: only discounted games count, a new sale after full price is a new key, hidden titles never light the dot
const wish = (id, discount, name = id) => ({ store: 'gog', id, name, priceInfo: discount == null ? null : { discount } })
assert.equal(h.wishlistKey(wish('7', 30)), 'gog:7:30')
h.state.wishlist = [wish('a', 30), wish('b', 0), wish('c', null), wish('d', 50, 'Hidden Game')]
h.state.settings.hidden = ['hidden game']
assert.equal(JSON.stringify(h.seenIds('wishlist')), '["gog:a:30","gog:d:50"]')
assert.equal(JSON.stringify(h.seenIds('wishlist', true)), '["gog:a:30"]')
assert.equal(JSON.stringify(h.mergeSeen({ wishlist: null }, 'wishlist', [], true)), '{"wishlist":[]}') // loaded, nothing on sale: seed empty
assert.equal(JSON.stringify([...h.newIds([h.wishlistKey(wish('a', 30))], ['gog:a:20'])]), '["gog:a:30"]')
h.state.wishlist = []; h.state.settings.hidden = []

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
h.state.settings.hidden = ['beta b']
assert.equal(JSON.stringify(h.filteredSorted(items, 'deals').map(x => x.title)), '["Alpha","Beta A","Gamma"]')
assert.equal(h.filteredSorted([{ name: 'Beta B', priceInfo: { discount: 80 } }], 'wishlist', { title: item => item.name, price: item => item.priceInfo.final, discount: item => item.priceInfo.discount }).length, 1)
h.state.settings.hidden = []
filters.sort = 'rating' // rating desc, unrated last, ties by discount
assert.equal(JSON.stringify(h.filteredSorted([{ title: 'U', cut: 99 }, { title: 'A', rating: 80, cut: 10 }, { title: 'B', rating: 95, cut: 5 }, { title: 'C', rating: 80, cut: 60 }], 'deals').map(x => x.title)), '["B","C","A","U"]')
filters.sort = 'discount'
assert.equal(h.mergeUiPrefs({ version: 1, lists: { deals: { sort: 'rating' }, wishlist: { sort: 'rating' } } }).lists.deals.sort, 'rating')
assert.equal(h.mergeUiPrefs({ version: 1, lists: { wishlist: { sort: 'rating' } } }).lists.wishlist.sort, 'discount')
assert.equal(JSON.stringify(h.dealShops([...items, { title: 'No shop' }])), '[["Fanatical",1],["GOG",1],["Steam",2]]')

context.now = new Date('2026-10-18T12:00:00Z')
assert.equal(vm.runInContext(nextSale, context).name, 'Winter Sale') // Steam Scream is a fest, not listed in SALES
h.state.lastChecked = Date.now() - 7 * 60_000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated 7 min ago')
h.state.lastChecked = Date.now()
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated just now')

console.log('Renderer self-check passed (escaping, hidden titles, search ranking, filters, store filter, UI preference merge, claimed games, seen deals, next sale, refresh age).')
