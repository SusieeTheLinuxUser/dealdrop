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
  between('const UI_PREFS_KEY', '\nconst initialUiPrefs'),
  'const initialUiPrefs = mergeUiPrefs(null)',
  between('const CLAIMED_KEY', '\nconst state = {'),
  between('const state = {', '\nlet shortcutsDialog'),
  between('function normalizeTitle (', '\nfunction announceStatus'),
  between('function storeKeyFor (', '\nconst gameStoreName'),
  between('function escHtml (', '\n// ─── Boot'),
  between('function seenIds (', '\nconst unseen'),
  between('function updateRefreshAge (', '\nfunction updateLastCheckedLabels'),
  between('const PAGE_SIZE', '\n// Rows for the first'),
  between('const BASE_THEMES = {', '\nfunction applyTheme'),
  between('function nextTheme (', '\nfunction syncSaleTheme'),
  between('function wishlistLowestTag (', '\n// ─── Price targets'),
  between('const saleKey =', '\n// Live sale →'),
  between('function updateAfterCheck (', '\n// "Update to v'),
  between('const targetKey', '\nfunction wishlistTargetTag'),
  between('const nextIndex', '\n// Moves focus between'),
  between('const fmt$ =', '\n// CSP forbids inline onerror'),
  'globalThis.helpers = { SALES, state, filteredSorted, storeKeyFor, escHtml, updateRefreshAge, mergeUiPrefs, dealShops, parseClaimed, pruneClaimed, toggleClaimed, claimedLast, normalizeTitle, hiddenTitle, searchRank, parseSeen, newIds, mergeSeen, wishlistKey, seenIds, salesSeenIds, PAGE_SIZE, pageSlice, nextTheme, targetKey, targetPrice, targetHit, parseTarget, protonTier, protonText, protonTag, nextIndex, fmt$, salePriceDescription, dataSig, timeLeft, endsFirst, endDateLabel, startDateLabel, saleDateLabel, fmtChecked, gameStoreName, srcIcon, wishlistLowestTag, saleKey, updateAfterCheck, BASE_THEMES, SALE_THEMES }',
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
assert.equal(JSON.stringify(h.parseClaimed(['a', 2, 'b'])), '["a","b"]')
assert.equal(JSON.stringify(h.parseClaimed({ a: 1 })), '[]')
assert.equal(JSON.stringify(h.parseClaimed('a')), '[]')
assert.equal(JSON.stringify(h.parseClaimed(null)), '[]')
assert.equal(JSON.stringify(h.toggleClaimed(['a'], 'b')), '["a","b"]')
assert.equal(JSON.stringify(h.toggleClaimed(['a', 'b'], 'a')), '["b"]')
assert.equal(JSON.stringify(h.pruneClaimed(['a', 'gone', 'b'], [{ id: 'b' }, { id: 'a' }, { id: 'c' }])), '["a","b"]')
assert.equal(JSON.stringify(h.claimedLast([{ id: 'a' }, { id: 'b' }, { id: 'c' }, { id: 'd' }], ['a', 'c']).map(g => g.id)), '["b","d","a","c"]')

assert.equal(JSON.stringify(h.state.seen), '{"deals":null,"storesales":null,"wishlist":null}') // no localStorage: first run, nothing tagged
assert.equal(JSON.stringify(h.parseSeen({ deals: ['a', 2], storesales: 'x' })), '{"deals":["a"],"storesales":null,"wishlist":null}')
assert.equal(JSON.stringify(h.parseSeen('deals')), '{"deals":null,"storesales":null,"wishlist":null}')
assert.equal(h.newIds(['a', 'b'], null).size, 0)
assert.equal(JSON.stringify([...h.newIds(['a', 'b', 'c'], ['b'])]), '["a","c"]')
assert.equal(JSON.stringify(h.mergeSeen({ deals: ['old'], storesales: null }, 'deals', ['a', 'b', 'a'])), '{"deals":["a","b"],"storesales":null}')
const seenBefore = { deals: ['a'], storesales: ['s'] }
assert.equal(h.mergeSeen(seenBefore, 'storesales', []), seenBefore) // empty list may be a failed load: keep the old set
// Store sales: leaving after the Steam tab marks only what it showed; unopened GOG keeps its old seen ids, its new sale stays new
assert.equal(h.state.salesViewed.size, 0)
const salesNow = { steam: [{ id: 's1' }, { id: 's2' }], gog: [{ id: 'g1' }, { id: 'g2' }], epic: [] }
const salesSeen = h.salesSeenIds(salesNow, new Set(['s1', 's2']), ['s1', 'g1', 'gone'])
assert.equal(JSON.stringify(salesSeen), '["s1","s2","g1"]')
assert.equal(JSON.stringify([...h.newIds(Object.values(salesNow).flat().map(d => d.id), salesSeen)]), '["g2"]')
assert.equal(JSON.stringify(h.salesSeenIds(salesNow, new Set(['s1', 's2', 'g1', 'g2']), [])), '["s1","s2","g1","g2"]')
// a Steam sale that arrived after the Steam tab was left (only GOG re-rendered) stays new
assert.equal(JSON.stringify(h.salesSeenIds({ ...salesNow, steam: [...salesNow.steam, { id: 's9' }] }, new Set(['s1', 's2', 'g1', 'g2']), [])), '["s1","s2","g1","g2"]')

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

// Paged lists: first `shown` items, the rest counted (never negative)
const many = Array.from({ length: 250 }, (_, i) => i)
assert.equal(h.PAGE_SIZE, 100)
assert.equal(JSON.stringify([h.pageSlice(many, 100).page.length, h.pageSlice(many, 100).left]), '[100,150]')
assert.equal(h.pageSlice(many, 200).page[199], 199)
assert.equal(JSON.stringify([h.pageSlice(many, 300).page.length, h.pageSlice(many, 300).left]), '[250,0]')
assert.equal(h.pageSlice([], 100).left, 0)

context.now = new Date('2026-10-18T12:00:00Z')
assert.equal(vm.runInContext(nextSale, context).name, 'Winter Sale') // Steam Scream is a fest, not listed in SALES
h.state.lastChecked = Date.now() - 7 * 60_000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated 7 min ago')
h.state.lastChecked = Date.now()
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated just now')

// Sale-theme lifecycle: each step feeds the last result back in, like settings saved across polls/restarts
let look = { theme: 'lavender', baseTheme: 'lavender', lastSaleTheme: '' }
const step = (key, pick, keep) => { if (pick) Object.assign(look, pick); look = { ...look, ...h.nextTheme(look, key, keep) }; return `${look.theme}/${look.lastSaleTheme}` }
assert.equal(step(''), 'lavender/')                                            // no sale
assert.equal(step(null), 'lavender/')                                          // detection failed: no change
assert.equal(step('winter'), 'winter/winter')                                  // sale starts: switch once
assert.equal(step('winter'), 'winter/winter')
assert.equal(step('winter', { theme: 'forest', baseTheme: 'forest' }), 'forest/winter') // user picks a base theme mid-sale
assert.equal(step('winter'), 'forest/winter')                                  // ...and it sticks across a restart
assert.equal(step(null), 'forest/winter')                                      // a failed check at startup doesn't reset it
assert.equal(step('winter'), 'forest/winter')
assert.equal(step('winter', { theme: 'winter' }), 'winter/winter')             // picks the sale theme again
assert.equal(step(''), 'forest/')                                              // sale ends: back to baseTheme
assert.equal(step('summer'), 'summer/summer')                                  // next different sale switches again
assert.equal(step('autumn'), 'autumn/autumn')                                  // a sale straight after another switches too
assert.equal(step('', { theme: 'summer' }), 'forest/')                         // ends while on another sale's theme: baseTheme
assert.equal(step('seasonal'), 'forest/')                                      // unknown sale with no theme: no change
assert.equal(step('lunar', null, true), 'forest/lunar')                        // tour theme pick wins, but the sale is recorded
assert.equal(step('lunar'), 'forest/lunar')
assert.equal(step('', { theme: 'lunar', baseTheme: 'gone' }), 'midnight/')     // unknown baseTheme falls back to midnight

// price targets: cents, $0.01–$10,000; compared with the cheaper offer, USD only
assert.equal(h.parseTarget('12.345'), 12.35)
assert.equal(h.parseTarget('10000'), 10000)
for (const bad of ['', '0', '0.004', '-1', '10000.01', 'abc', 'Infinity']) assert.equal(h.parseTarget(bad), null, bad)

h.state.proton = { 440: 'gold', 7: '<img src=x>', 8: 'constructor' }
assert.equal(h.protonTier('440'), 'gold')
assert.equal(h.protonText('440'), 'ProtonDB: Gold')
assert.match(h.protonTag('440'), /class="proton-tag gold".*🐧 Gold.*ProtonDB: Gold\./)
for (const id of [null, undefined, '9', '7', '8', 'constructor']) assert.equal(h.protonTag(id), '', String(id)) // unknown or unexpected tiers never render
h.state.proton = {}
const targeted = { store: 'gog', id: 'g1', priceInfo: { final: 15, currency: 'USD' }, cheapest: { price: 9.5 } }
assert.equal(h.targetKey(targeted), 'gog:g1')
assert.equal(h.targetPrice(targeted), 9.5)
assert.equal(h.targetHit(targeted, 9.5), true)
assert.equal(h.targetHit(targeted, 9.49), false)
assert.equal(h.targetHit({ ...targeted, priceInfo: { final: 5, currency: 'EUR' } }, 10), false)
assert.equal(h.targetHit(targeted, undefined), false)

assert.equal(h.nextIndex(-1, 5, 1), 0)    // J with nothing focused: first item
assert.equal(h.nextIndex(-1, 5, -1), 4)   // K with nothing focused: last item
assert.equal(h.nextIndex(2, 5, 1), 3)
assert.equal(h.nextIndex(2, 5, -1), 1)
assert.equal(h.nextIndex(4, 5, 1), 4)     // stops at the ends
assert.equal(h.nextIndex(0, 5, -1), 0)
assert.equal(h.nextIndex(-1, 0, 1), -1)   // empty list

// Money and dates as they are shown on a card
assert.equal(h.fmt$(0), 'Free')
assert.equal(h.fmt$(4.999), '$5.00')
assert.equal(h.fmt$('n/a'), 'Free')
assert.equal(h.salePriceDescription(19.99, 4.99, 75, ''), 'Was $19.99, now $4.99, 75% off')
assert.equal(h.salePriceDescription(19.99, 4.99, 75, '$4.99'), 'Was $19.99, now $4.99, 75% off')
assert.equal(h.salePriceDescription(19.99, 4.99, 75, '4.99 EUR'), 'Was $19.99, now 4.99 EUR, 75% off')
assert.equal(h.salePriceDescription(0, 0, 100, ''), 'Was Free, now Free, 100% off')

// How long a giveaway still has: days, then hours, then "soon"
const later = ms => new Date(Date.now() + ms).toISOString()
assert.equal(h.timeLeft(''), '')
assert.equal(h.timeLeft(later(-1000)), 'ended')
assert.equal(h.timeLeft(later(2.5 * 86400000)), '2d left')
assert.equal(h.timeLeft(later(3.5 * 3600000)), '3h left')
assert.equal(h.timeLeft(later(30 * 60000)), 'ending soon')
assert.equal(h.endDateLabel(later(-1000)), 'Ended')
assert.equal(h.endDateLabel(later(30 * 60000)), 'Ends soon')
assert.equal(h.endDateLabel(later(2.5 * 86400000)), 'Ends in 2d')
assert.equal(h.endDateLabel(later(3.5 * 3600000)), 'Ends in 3h')

// The soonest end date first; a row without one goes last
const dated = [{ id: 'a', endDate: '2026-10-20T00:00:00Z' }, { id: 'b', endDate: '' }, { id: 'c', endDate: '2026-10-22T00:00:00Z' }, { id: 'd', endDate: 'nope' }]
assert.equal(JSON.stringify(h.endsFirst(dated).map(x => x.id)), '["a","c","b","d"]')
assert.equal(JSON.stringify(dated.map(x => x.id)), '["a","b","c","d"]') // the list is not sorted in place

// An upcoming giveaway: the date is only in the label, so the format is the browser's
assert.equal(h.startDateLabel(''), 'Free soon')
assert.equal(h.startDateLabel('nope'), 'Free soon')
assert.equal(h.startDateLabel('2020-01-01T00:00:00Z'), 'Free now')
assert.equal(h.startDateLabel('2030-01-01T00:00:00Z').startsWith('Free from '), true)

assert.equal(h.fmtChecked(0), '')
// The sales calendar prints the listed day in every timezone (UTC midnight is the day before west of Greenwich)
const tz = process.env.TZ
for (const zone of ['America/Los_Angeles', 'Pacific/Auckland', 'UTC']) {
  process.env.TZ = zone
  assert.equal(h.saleDateLabel(new Date('2026-12-17'), new Date('2026-10-18T12:00:00Z')), 'Dec 17', zone)
  assert.equal(h.saleDateLabel(new Date('2027-01-04'), new Date('2026-10-18T12:00:00Z')), 'Jan 4, 2027', zone)
}
if (tz === undefined) delete process.env.TZ; else process.env.TZ = tz
assert.equal(h.fmtChecked(Date.now()), 'updated just now')
assert.match(h.fmtChecked(Date.now() - 30000), /^updated 3[01]s ago$/) // the clock may tick between the two reads
assert.equal(h.fmtChecked(Date.now() - 5 * 60000), 'updated 5m ago')
assert.equal(h.fmtChecked(Date.now() - 3 * 3600000), 'updated 3h ago')

// Edge cases: values the backend should never send must not read as a price, a time or a match
assert.equal(h.timeLeft('not a date'), '') // an unparseable end date is not "ending soon"
assert.equal(h.endDateLabel('not a date'), '')
assert.equal(h.endDateLabel(''), '')
assert.equal(h.timeLeft(later(86400000 - 1000)), '23h left')
assert.equal(h.timeLeft(later(86400000 + 1000)), '1d left')
assert.equal(h.timeLeft(later(3600000 - 1000)), 'ending soon')
assert.equal(h.fmt$(null), 'Free')
assert.equal(h.fmt$(undefined), 'Free')
assert.equal(h.fmt$(NaN), 'Free')
assert.equal(h.fmt$('4.5'), '$4.50')
assert.equal(h.fmt$(0.01), '$0.01')
assert.equal(h.fmt$(1234.5), '$1234.50')
assert.equal(h.fmt$(10000), '$10000.00')
assert.equal(h.fmt$(-0), 'Free')
const nullFinal = { store: 'steam', id: '1', priceInfo: { final: null, currency: 'USD' }, cheapest: null }
assert.equal(h.targetPrice(nullFinal), null) // NaN serialized as null used to become Math.min(null, …) = 0
assert.equal(h.targetHit(nullFinal, 10), false)
assert.equal(h.targetPrice({ priceInfo: { final: 5, currency: 'USD' }, cheapest: { price: null } }), 5)
assert.equal(h.targetPrice({ priceInfo: null }), null)
assert.equal(h.targetHit({ priceInfo: { final: 10.005, currency: 'USD' } }, 10), true)
assert.equal(h.targetHit({ priceInfo: { final: 10.006, currency: 'USD' } }, 10), false)
assert.equal(h.parseTarget('0.005'), 0.01)
assert.equal(h.parseTarget(' 5 '), 5)
for (const bad of [null, undefined, NaN, '1e999', '-0']) assert.equal(h.parseTarget(bad), null, String(bad))
assert.equal(h.pageSlice(many, 0).left, 250)
assert.equal(h.pageSlice(many, 250).left, 0)
assert.equal(h.pageSlice(many, 249).left, 1)
assert.equal(h.pageSlice(many, Infinity).page.length, 250)
// unicode in search: lowercasing is per code point, runs of any whitespace collapse, accents are not folded
const uni = ['Ünïcode Quest', 'ÜNÏCODE', '日本語 ゲーム', 'Straße', 'עברית game', '🎮  Emoji   Game', 'café'].map(title => ({ title }))
assert.equal(JSON.stringify(h.searchRank(uni, 'ünï').map(g => g.title)), '["Ünïcode Quest","ÜNÏCODE"]')
assert.equal(JSON.stringify(h.searchRank(uni, '日本語  ゲーム').map(g => g.title)), '["日本語 ゲーム"]')
assert.equal(JSON.stringify(h.searchRank(uni, 'STRASSE').map(g => g.title)), '[]') // no ß folding
assert.equal(JSON.stringify(h.searchRank(uni, '🎮 emoji').map(g => g.title)), '["🎮  Emoji   Game"]')
assert.equal(JSON.stringify(h.searchRank(uni, 'עברית').map(g => g.title)), '["עברית game"]')
assert.equal(h.searchRank(uni, 'cafe').length, 0)
assert.equal(h.searchRank([{ title: null }, {}, { title: 5 }], '5').length, 1)
assert.equal(h.searchRank(uni, 'e', 0).length, 0)
// the whole title is one match, even when another row holds it twice
assert.equal(h.searchRank([{ title: 'aa' }, { title: 'aa' }], 'a').length, 2)
// seen bookkeeping: duplicates collapse, an empty list that did load clears the set, ids compare as exact strings
assert.equal(JSON.stringify(h.mergeSeen({ deals: ['x'] }, 'deals', [], true)), '{"deals":[]}')
assert.equal(JSON.stringify([...h.newIds(['10', '1'], ['1'])]), '["10"]')
assert.equal(JSON.stringify([...h.newIds(['a', 'a'], [])]), '["a"]')
assert.equal(JSON.stringify(h.parseSeen({ deals: [] })), '{"deals":[],"storesales":null,"wishlist":null}')
assert.equal(JSON.stringify(h.parseSeen([1])), '{"deals":null,"storesales":null,"wishlist":null}')
assert.equal(JSON.stringify(h.parseSeen(undefined)), '{"deals":null,"storesales":null,"wishlist":null}')
assert.equal(JSON.stringify(h.parseSeen(null)), '{"deals":null,"storesales":null,"wishlist":null}')
// refresh age: a clock set back (future timestamp) is "just now"; hours read as minutes
h.state.lastChecked = Date.now() + 3600000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated just now')
h.state.lastChecked = Date.now() - 125 * 60_000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated 125 min ago')
for (const bad of [0, -5, NaN, 'x', null]) { h.state.lastChecked = bad; h.updateRefreshAge(); assert.equal(label.textContent, 'Never', String(bad)) }
assert.equal(h.fmtChecked(Date.now() + 5000), 'updated just now')
assert.equal(h.fmtChecked(Date.now() - 59 * 60000), 'updated 59m ago')
assert.equal(h.fmtChecked(Date.now() - 3600000), 'updated 1h ago')
// sale price text keeps the store's own formatted price and survives odd inputs
assert.equal(h.salePriceDescription(1234.5, 0.99, 99, ''), 'Was $1234.50, now $0.99, 99% off')
assert.equal(h.salePriceDescription(NaN, NaN, 0, null), 'Was Free, now Free, 0% off')
// J/K from a stale position beyond the list still lands on the last row
assert.equal(h.nextIndex(9, 3, 1), 2)
assert.equal(h.nextIndex(9, 3, -1), 2)

assert.equal(h.gameStoreName('steam'), 'Steam')
assert.equal(h.gameStoreName('gog'), 'GOG')
assert.equal(h.gameStoreName('epic'), 'Epic Games')
assert.equal(h.gameStoreName('other'), 'other')
assert.equal(h.gameStoreName(), 'Store')
assert.equal(h.srcIcon('steam'), '🎮')
assert.equal(h.srcIcon('epic'), '🕹️')
assert.equal(h.srcIcon('gog'), '⚔️')
assert.equal(h.srcIcon('other'), '🎮')

// A poll with the same data but a later check time is "unchanged"; any data difference is not
assert.equal(h.dataSig({ deals: [1], lastChecked: 1 }), h.dataSig({ deals: [1], lastChecked: 2 }))
assert.notEqual(h.dataSig({ deals: [1], lastChecked: 1 }), h.dataSig({ deals: [2], lastChecked: 1 }))

// Escaping: every remote string that goes near innerHTML; `'` passes through because every
// attribute interpolation in app.js is double-quoted (the tripwire below fails if one ever isn't)
assert.equal(h.escHtml(`<tag a="&">'`), '&lt;tag a=&quot;&amp;&quot;&gt;\'')
assert.equal(h.escHtml(`<b>&"'</b>`), '&lt;b&gt;&amp;&quot;\'&lt;/b&gt;')
assert.equal(h.escHtml('Hades & Co'), 'Hades &amp; Co')
assert.equal(h.escHtml(''), '')
assert.equal(h.escHtml(null), '')
assert.equal(h.escHtml(5), '5')
assert.equal(/='\$\{[^}]*escHtml\(/.test(app), false) // no single-quoted attribute carries an escaped value

// ProtonDB: all five tiers render (capitalised, on the 🐧 tag); anything else never does
h.state.proton = { 1: 'platinum', 2: 'gold', 3: 'silver', 4: 'bronze', 5: 'borked', 6: 'pending', 7: 'Platinum' }
for (const [id, tier] of Object.entries(h.state.proton).slice(0, 5)) {
  assert.match(h.protonTag(id), new RegExp(`class="proton-tag ${tier}".*🐧 ${tier[0].toUpperCase() + tier.slice(1)}`), tier)
  assert.equal(h.protonText(id), `ProtonDB: ${tier[0].toUpperCase() + tier.slice(1)}`)
}
for (const id of [6, 7]) assert.equal(h.protonTag(String(id)), '', `tier ${h.state.proton[id]} must not render`) // pending and wrong-case are unknown
h.state.proton = {}

// "Lowest ever": USD store prices at their all-time low, with a cent of slack
const low = (final, lowest, currency = 'USD', discount = 50) => h.wishlistLowestTag({ lowest, priceInfo: { final, lowest, currency, discount } })
assert.match(low(4.99, 4.99), /Lowest ever/)
assert.match(low(4.995, 4.99), /Lowest ever/)  // a cent above the low still counts
assert.equal(low(5.00, 4.99), '')              // more than that: no tag
assert.equal(low(4.99, 4.99, 'EUR'), '')       // CheapShark's low is USD: only USD compares
assert.equal(low(4.99, 4.99, 'USD', 0), '')    // not on sale: the tag is only for sales
assert.equal(low(4.99, null), '')

// Calendar: the sale name carries its theme key
assert.equal(h.saleKey({ name: 'Steam Winter Sale' }), 'winter')
assert.equal(h.saleKey({ name: 'Lunar New Year Sale' }), 'lunar')
assert.equal(h.saleKey({ name: 'Autumn Sale' }), 'autumn')
assert.equal(h.saleKey({ name: 'Weekly Deals' }), undefined)
assert.equal(h.saleKey({ name: 'Steam Scream Fest' }), undefined) // a fest, not a seasonal sale

// A failed update check keeps the offer already shown; only a reached answer replaces it
const offered = { version: '0.10.0' }
assert.equal(h.updateAfterCheck(offered, null, false), offered)
assert.equal(h.updateAfterCheck(null, null, false), null)
assert.equal(h.updateAfterCheck(offered, null, true), null) // reached, and nothing newer: the offer is gone
assert.equal(h.updateAfterCheck(null, offered, true), offered)

// Countdowns: the d/h/soon split is the floor of the exact instant delta (never a wall-clock
// day count, which would shift across a DST night); relative durations keep this runnable any day
for (const [ms, label] of [[2.5 * 86400000, '2d left'], [86400000 + 60000, '1d left'], [86400000 - 60000, '23h left'], [5 * 3600000 - 60000, '4h left'], [3600000 + 60000, '1h left'], [3600000 - 60000, 'ending soon']]) {
  assert.equal(h.timeLeft(new Date(Date.now() + ms).toISOString()), label, String(ms))
}
assert.equal(h.timeLeft(new Date(Date.now() - 86400000).toISOString()), 'ended') // a day past, not a negative label
assert.equal(h.timeLeft('2026-12-31'), h.timeLeft('2026-12-31T00:00:00Z')) // a date-only string is UTC midnight
assert.equal(h.timeLeft('9999-12-31T00:00:00Z').endsWith('d left'), true)  // a far future date is still days
assert.equal(h.endDateLabel('2020-01-01T00:00:00Z'), 'Ended')              // long past, not "ending soon"

// Themes: every theme the JS offers has a CSS block (midnight is the :root default), and CSS has none the JS lacks
const css = readFileSync(new URL('../../renderer/style.css', import.meta.url), 'utf8')
const cssThemes = new Set([...css.matchAll(/html\[data-theme="([a-z]+)"\]/g)].map(m => m[1]))
const jsThemes = new Set([...Object.keys(h.BASE_THEMES), ...Object.keys(h.SALE_THEMES)])
for (const t of jsThemes) {
  if (t === 'midnight') { assert.equal(cssThemes.has('midnight'), false, 'midnight should be the :root default, not its own block'); continue }
  assert.equal(cssThemes.has(t), true, `theme "${t}" has no html[data-theme] block in style.css`)
}
for (const t of cssThemes) assert.equal(jsThemes.has(t), true, `style.css theme "${t}" is missing from BASE_THEMES/SALE_THEMES in app.js`)
assert.equal(jsThemes.has('midnight'), true) // and the root default is offered in Settings

console.log('Renderer self-check passed (price targets, escaping, hidden titles, search ranking, filters, store filter, UI preference merge, claimed games, seen deals, list paging, next sale, refresh age, sale themes, J/K navigation, money and date labels, poll data signature, ProtonDB tiers, lowest-ever rule, sale keys, countdown thresholds, theme/CSS consistency, failed update check).')
