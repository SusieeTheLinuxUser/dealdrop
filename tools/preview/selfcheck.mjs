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
  between('const DEFAULT_SETTINGS = {', '\nconst state = {'),
  between('const state = {', '\nlet shortcutsDialog'),
  between('function filteredSorted (', '\nfunction syncMinDiscount'),
  between('function escHtml (', '\n// ─── Boot'),
  between('function updateRefreshAge (', '\nfunction updateLastCheckedLabels'),
  'globalThis.helpers = { SALES, state, filteredSorted, escHtml, updateRefreshAge }',
].join('\n'), context)

const h = context.helpers
assert.equal(h.escHtml(`<tag a="&">'`), '&lt;tag a=&quot;&amp;&quot;&gt;\'')

const items = [
  { title: 'Beta B', price: 8, cut: 50 }, { title: 'Alpha', price: 5, cut: 90 },
  { title: 'Beta A', price: 3, cut: 10 }, { title: 'Gamma', price: 2, cut: 0 },
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

context.now = new Date('2026-10-18T12:00:00Z')
assert.equal(vm.runInContext(nextSale, context).name, 'Halloween Sale')
h.state.lastChecked = Date.now() - 7 * 60_000
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated 7 min ago')
h.state.lastChecked = Date.now()
h.updateRefreshAge()
assert.equal(label.textContent, 'Updated just now')

console.log('Renderer self-check passed (escaping, filters, next sale, refresh age).')
