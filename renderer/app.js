'use strict'

// Dates: Steamworks "Upcoming Steam Events". `end` = announced last day; est = not announced yet.
// Steam Scream (26 Oct–2 Nov 2026) is a horror fest, not a seasonal sale, so it is not listed.
const SALES = [
  { name: 'Spring Sale', start: '2026-03-19', end: '2026-03-26', icon: '🌸' },
  { name: 'Summer Sale', start: '2026-06-25', end: '2026-07-09', icon: '☀️' },
  { name: 'Autumn Sale', start: '2026-10-01', end: '2026-10-08', icon: '🍂' },
  { name: 'Winter Sale', start: '2026-12-17', end: '2027-01-04', icon: '❄️' },
  { name: 'Spring Sale', start: '2027-03-18', end: '2027-03-25', icon: '🌸' },
  { name: 'Summer Sale', start: '2027-06-24', end: '2027-07-08', icon: '☀️' },
  { name: 'Autumn Sale', start: '2027-10-07', end: '2027-10-14', icon: '🍂', est: true },
]

const DEFAULT_SETTINGS = {
  notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true,
  notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60,
  theme: 'midnight', baseTheme: 'midnight', lastSaleTheme: '', rounded: false, effects: true,
  launcherGog: 'auto', launcherEpic: 'auto', onboarded: false,
}

const UI_PREFS_KEY = 'dealdrop-ui'
const UI_PREFS_DEFAULTS = {
  version: 1, page: 'free',
  lists: { deals: { sort: 'discount', minDiscount: 0 }, storesales: { sort: 'discount', minDiscount: 0 }, wishlist: { sort: 'discount', onSaleOnly: false } },
}

function mergeUiPrefs (saved) {
  const prefs = JSON.parse(JSON.stringify(UI_PREFS_DEFAULTS))
  if (saved?.version !== prefs.version) return prefs
  if (['wishlist', 'free', 'storesales', 'deals', 'sales', 'settings'].includes(saved.page)) prefs.page = saved.page
  for (const key of Object.keys(prefs.lists)) {
    const value = saved.lists?.[key]
    if (['discount', 'price', 'name'].includes(value?.sort)) prefs.lists[key].sort = value.sort
    if (key !== 'wishlist' && [0, 50, 75, 90].includes(value?.minDiscount)) prefs.lists[key].minDiscount = value.minDiscount
    if (key === 'wishlist' && typeof value?.onSaleOnly === 'boolean') prefs.lists[key].onSaleOnly = value.onSaleOnly
  }
  return prefs
}

function readUiPrefs () {
  try { return mergeUiPrefs(JSON.parse(localStorage.getItem(UI_PREFS_KEY) || 'null')) } catch { return mergeUiPrefs(null) }
}

const initialUiPrefs = readUiPrefs()

function persistUiPrefs () {
  const { deals, storesales, wishlist } = state.listFilters
  try {
    localStorage.setItem(UI_PREFS_KEY, JSON.stringify({ version: 1, page: state.page, lists: {
      deals: { sort: deals.sort, minDiscount: deals.minDiscount },
      storesales: { sort: storesales.sort, minDiscount: storesales.minDiscount },
      wishlist: { sort: wishlist.sort, onSaleOnly: wishlist.onSaleOnly },
    } }))
  } catch {}
}

function clearUiPrefs () {
  try { localStorage.removeItem(UI_PREFS_KEY) } catch {}
}

const state = {
  page:         initialUiPrefs.page,
  onboardingOpen: false,
  onboardingThemePicked: false,
  freeGames:    [],
  freeWeekends: [],
  deals:        [],
  dealsSource:  '',
  dealsError:   null,
  sales:        { steam: [], gog: [], epic: [] },
  liveSale:     null,
  launchers:    {},   // store -> { options:[{key,label}], chosen:{key,label}|null } from the backend
  salesTab:     'steam',
  wishlistTab:  'all',
  wishlistErrors: {},
  wishlist:     [],
  settings:     { ...DEFAULT_SETTINGS },
  accounts:     { steam: null, gog: null, epic: null },
  sourceErrors: [],
  fetchError:   null,
  dataLoaded:   false,
  lastChecked:  null,
  selectedWishlistDeal: null,
  wishlistPrivate: false,
  refreshing: false,
  listFilters: {
    deals: { query: '', ...initialUiPrefs.lists.deals }, storesales: { query: '', ...initialUiPrefs.lists.storesales },
    wishlist: { query: '', ...initialUiPrefs.lists.wishlist },
  },
}
let shortcutsDialog = null
let refreshAgeInterval = null

function filteredSorted (items, key, fields = {}) {
  const { query, sort, onSaleOnly, minDiscount = 0 } = state.listFilters[key]
  const value = (item, field, defaultField) => fields[field] ? fields[field](item) : item[defaultField]
  const q = query.trim().toLocaleLowerCase()
  const discount = item => Number(value(item, 'discount', 'cut')) || 0
  const price = item => {
    const amount = Number(value(item, 'price', 'price'))
    return Number.isFinite(amount) ? amount : Infinity
  }
  return items.filter(item => {
    const title = String(value(item, 'title', 'title') ?? '')
    return (!q || title.toLocaleLowerCase().includes(q)) && discount(item) >= Math.max(minDiscount, onSaleOnly ? 1 : 0)
  }).sort((a, b) => {
    if (sort === 'price') return price(a) - price(b)
    if (sort === 'name') return String(value(a, 'title', 'title') ?? '').localeCompare(String(value(b, 'title', 'title') ?? ''))
    return discount(b) - discount(a)
  })
}

function announceStatus (message) {
  const status = document.getElementById('app-status')
  if (status) status.textContent = message
}

function renderSkeletons (container, kind) {
  container.replaceChildren()
  for (let i = 0; i < 3; i++) {
    const item = document.createElement('div')
    item.className = kind === 'card' ? 'skeleton-card' : 'skeleton-row'
    if (kind === 'card') item.append(document.createElement('span'), document.createElement('span'), document.createElement('span'))
    else item.append(document.createElement('span'), document.createElement('span'))
    item.classList.add('skeleton')
    container.appendChild(item)
  }
  const label = document.createElement('span')
  label.className = 'sr-only'; label.textContent = 'Loading…'
  container.appendChild(label)
}

function currentListItems (key) {
  if (key === 'deals') return state.deals
  if (key === 'storesales') return state.sales[state.salesTab] ?? []
  return state.wishlistTab === 'all' ? state.wishlist : state.wishlist.filter(item => item.store === state.wishlistTab)
}

function announceListResults (key) {
  const fields = key === 'wishlist' ? { title: item => item.name, price: item => item.priceInfo?.final ?? Infinity, discount: item => item.priceInfo?.discount ?? 0 } : {}
  const count = filteredSorted(currentListItems(key), key, fields).length
  announceStatus(`${count} ${key === 'wishlist' ? 'wishlist games' : 'deals'} shown`)
}

function updateResultCount (key, visible, total) {
  const count = document.getElementById(`${key}-result-count`)
  if (!count) return
  const filters = state.listFilters[key]
  const active = state.dataLoaded && (!!filters.query.trim() || !!filters.minDiscount || !!filters.onSaleOnly)
  count.hidden = !active
  count.textContent = `${visible} of ${total}`
}

function syncMinDiscount (key, controlId) {
  const minDiscount = state.listFilters[key].minDiscount || 0
  document.querySelectorAll(`#${controlId} [data-min-discount]`).forEach(button => {
    button.setAttribute('aria-pressed', String(Number(button.dataset.minDiscount) === minDiscount))
  })
}

function bindListControls (key, inputId, sortId, render, { saleOnlyId, minDiscountId } = {}) {
  const input = document.getElementById(inputId)
  const sort = document.getElementById(sortId)
  let timer
  const update = () => { render(); persistUiPrefs(); announceListResults(key) }
  input.addEventListener('input', () => {
    state.listFilters[key].query = input.value
    clearTimeout(timer); timer = setTimeout(update, 180)
  })
  sort.addEventListener('change', () => { state.listFilters[key].sort = sort.value; update() })
  if (saleOnlyId) {
    const toggle = document.getElementById(saleOnlyId)
    toggle.addEventListener('click', () => {
      state.listFilters[key].onSaleOnly = !state.listFilters[key].onSaleOnly
      toggle.setAttribute('aria-pressed', String(state.listFilters[key].onSaleOnly))
      update()
    })
  }
  if (minDiscountId) {
    const controls = document.getElementById(minDiscountId)
    syncMinDiscount(key, minDiscountId)
    controls.addEventListener('click', e => {
      const button = e.target.closest('[data-min-discount]')
      if (!button) return
      state.listFilters[key].minDiscount = Number(button.dataset.minDiscount)
      syncMinDiscount(key, minDiscountId)
      update()
    })
  }
}

function showFilterEmpty (emptyEl, key, inputId, minDiscountId, render, message, { saleOnlyId } = {}) {
  const copy = emptyEl.querySelector('#wishlist-empty-msg') || document.createElement('p')
  copy.textContent = message
  const clear = document.createElement('button')
  clear.type = 'button'; clear.className = 'clear-filter'; clear.textContent = 'Clear filters'
  clear.addEventListener('click', () => {
    state.listFilters[key].query = ''
    if ('minDiscount' in state.listFilters[key]) state.listFilters[key].minDiscount = 0
    if ('onSaleOnly' in state.listFilters[key]) state.listFilters[key].onSaleOnly = false
    document.getElementById(inputId).value = ''
    if (minDiscountId) syncMinDiscount(key, minDiscountId)
    if (saleOnlyId) document.getElementById(saleOnlyId).setAttribute('aria-pressed', 'false')
    render()
    persistUiPrefs(); announceListResults(key)
  })
  emptyEl.replaceChildren(copy, clear)
}

// ─── Utilities ────────────────────────────────────────────────────────────────

const fmt$ = n => {
  const num = Number(n) || 0
  return num === 0 ? 'Free' : '$' + num.toFixed(2)
}

const salePriceDescription = (regular, price, discount, formatted) =>
  `Was ${fmt$(regular)}, now ${formatted || fmt$(price)}, ${discount}% off`

function timeLeft (dateStr) {
  if (!dateStr) return ''
  const diff = new Date(dateStr) - Date.now()
  if (diff <= 0) return 'ended'
  const d = Math.floor(diff / 86400000)
  const h = Math.floor((diff % 86400000) / 3600000)
  if (d > 0) return `${d}d left`
  if (h > 0) return `${h}h left`
  return 'ending soon'
}

function endsFirst (items) {
  return [...items].sort((a, b) => {
    const aEnd = Date.parse(a.endDate || '')
    const bEnd = Date.parse(b.endDate || '')
    const aTime = Number.isFinite(aEnd) ? aEnd : Infinity
    const bTime = Number.isFinite(bEnd) ? bEnd : Infinity
    return aTime - bTime
  })
}

function endDateLabel (dateStr) {
  const remaining = timeLeft(dateStr)
  if (remaining === 'ended') return 'Ended'
  if (remaining === 'ending soon') return 'Ends soon'
  return `Ends in ${remaining.replace(/ left$/, '')}`
}

function fmtChecked (ts) {
  if (!ts) return ''
  const s = Math.floor((Date.now() - ts) / 1000)
  if (s < 10)   return 'updated just now'
  if (s < 60)   return `updated ${s}s ago`
  if (s < 3600) return `updated ${Math.floor(s / 60)}m ago`
  return `updated ${Math.floor(s / 3600)}h ago`
}

const srcIcon = src => ({ steam: '🎮', epic: '🕹️', gog: '⚔️' }[src] ?? '🎮')
const STORES = { steam: 'Steam', gog: 'GOG', epic: 'Epic' }
function storeKeyFor (store) {
  const value = String(store ?? '').toLowerCase()
  return ['steam', 'gog', 'epic'].find(key => value.includes(key)) ?? value
}
const gameStoreName = store => ({steam:'Steam', gog:'GOG', epic:'Epic Games'}[String(store ?? '').toLowerCase()] ?? String(store ?? 'Store'))
const hasAccount = () => Object.values(state.accounts).some(Boolean)
const onSaleItems = () => state.wishlist.filter(w => (w.priceInfo?.discount ?? 0) >= 20)

// CSP forbids inline onerror handlers, so broken <img>s are swapped for a glyph here
document.addEventListener('error', e => {
  const img = e.target
  if (img.tagName !== 'IMG') return
  const span = document.createElement('span')
  span.className = 'fallback'
  span.textContent = img.dataset.fb ?? ''
  img.replaceWith(span)
}, true)

function tmpl (html) {
  const t = document.createElement('template')
  t.innerHTML = html.trim()
  return t.content.firstChild
}

// ─── Open helpers ─────────────────────────────────────────────────────────────

const REPO       = 'https://github.com/SusieeTheLinuxUser/dealdrop'
const openBrowser = url   => window.api.openUrl(url)
const openSteam   = appId => window.api.openInApp('steam', `https://store.steampowered.com/app/${appId}`)
// Heroic and Lutris open an install dialog, not a store page, so GOG cards only get GOG Galaxy (else just Browser).
const launcherOf  = store => { const ch = state.launchers[store]?.chosen ?? null; return store === 'gog' && ch?.key !== 'native' ? null : ch }

function closeShortcutsDialog () {
  if (!shortcutsDialog) return
  const { overlay, returnFocus } = shortcutsDialog
  shortcutsDialog = null
  overlay.remove()
  returnFocus?.focus?.()
}

function openShortcutsDialog () {
  if (shortcutsDialog) return
  const returnFocus = document.activeElement
  const overlay = document.createElement('div'); overlay.className = 'shortcut-backdrop'
  const dialog = document.createElement('section')
  dialog.className = 'shortcut-dialog'; dialog.setAttribute('role', 'dialog'); dialog.setAttribute('aria-modal', 'true')
  dialog.setAttribute('aria-labelledby', 'shortcut-dialog-title')
  const title = document.createElement('h2'); title.id = 'shortcut-dialog-title'; title.textContent = 'Keyboard shortcuts'
  const list = document.createElement('dl'); list.className = 'shortcut-list'
  ;[
    ['R', 'Refresh all'], ['/','Focus search on this page'],
    ['1–5', 'Open Wishlist, Free now, Store sales, Hot deals, or Calendar'],
    [',', 'Open Settings'], ['?', 'Show this help'],
  ].forEach(([key, description]) => {
    const term = document.createElement('dt'), code = document.createElement('kbd'), detail = document.createElement('dd')
    code.textContent = key; term.appendChild(code); detail.textContent = description; list.append(term, detail)
  })
  const close = document.createElement('button'); close.type = 'button'; close.className = 'shortcut-close'; close.textContent = 'Close'
  close.addEventListener('click', closeShortcutsDialog)
  dialog.append(title, list, close); overlay.appendChild(dialog); document.body.appendChild(overlay)
  shortcutsDialog = { overlay, returnFocus }
  close.focus()
}

function handleShortcutKeydown (e) {
  if (shortcutsDialog) {
    if (e.key === 'Escape') { e.preventDefault(); closeShortcutsDialog(); return }
    if (e.key === 'Tab') { e.preventDefault(); shortcutsDialog.overlay.querySelector('button').focus() }
    return
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return
  if (e.target.isContentEditable || e.target.closest?.('input, textarea, select, [contenteditable="true"]')) return
  if (document.querySelector('.onboarding-backdrop, .tour-backdrop')) return
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key
  if (key === '?') { e.preventDefault(); openShortcutsDialog(); return }
  if (key === 'r') { e.preventDefault(); loadData(true); return }
  if (key === '/') {
    const id = { deals: 'deals-search', storesales: 'sales-search', wishlist: 'wishlist-search' }[state.page]
    const search = id && document.getElementById(id)
    if (search?.getClientRects().length) { e.preventDefault(); search.focus() }
    return
  }
  if (key === ',') { e.preventDefault(); navigate('settings'); return }
  const pages = ['wishlist', 'free', 'storesales', 'deals', 'sales']
  if (/^[1-5]$/.test(key)) { e.preventDefault(); navigate(pages[Number(key) - 1]) }
}

function updateRefreshAge () {
  const label = document.getElementById('refresh-age')
  if (!label) return
  const timestamp = Number(state.lastChecked)
  if (!Number.isFinite(timestamp) || timestamp <= 0) { label.textContent = 'Never'; return }
  const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000))
  label.textContent = minutes === 0 ? 'Updated just now' : `Updated ${minutes} min ago`
}

function updateLastCheckedLabels () {
  updateRefreshAge()
  const label = document.getElementById('lc-' + state.page)
  if (label && state.lastChecked) label.textContent = fmtChecked(state.lastChecked)
}

function stopRefreshAgeTimer () {
  if (refreshAgeInterval !== null) clearInterval(refreshAgeInterval)
  refreshAgeInterval = null
}

function startRefreshAgeTimer () {
  updateLastCheckedLabels()
  if (refreshAgeInterval !== null) return
  refreshAgeInterval = setInterval(updateLastCheckedLabels, 30_000)
  window.addEventListener('pagehide', stopRefreshAgeTimer, { once: true })
}

// ─── Shared action buttons ────────────────────────────────────────────────────

// Buttons: "<Store> app" only when that launcher is installed (the backend falls back
// to the browser anyway), plus Browser.
function makeCardActions (store, url, id, title) {
  const wrap     = document.createElement('div')
  wrap.className = 'card-actions'
  const ch = launcherOf(store)
  if (ch) {
    const appBtn = document.createElement('button')
    appBtn.type = 'button'; appBtn.className = 'card-btn steam-btn'; appBtn.title = `Open in ${ch.label}`; appBtn.textContent = ch.label
    appBtn.setAttribute('aria-label', `Open ${title} in ${ch.label}`)
    appBtn.addEventListener('click', e => { e.stopPropagation(); window.api.openInApp(store, url, id) })
    wrap.append(appBtn)
  }
  const webBtn = document.createElement('button')
  webBtn.type = 'button'; webBtn.className = 'card-btn'; webBtn.title = 'Open in browser'; webBtn.textContent = '🌐 Browser'
  webBtn.setAttribute('aria-label', `Open ${title} in your browser`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })
  wrap.append(webBtn)
  return wrap
}

function makeCardOpenButton (store, url, title, description = '') {
  const button = document.createElement('button')
  button.type = 'button'; button.className = 'game-card-open'
  const key = storeKeyFor(store)
  const label = ['steam', 'gog', 'epic'].includes(key) ? `Open ${title} on ${gameStoreName(key)}` : `Open ${title} in your browser`
  button.setAttribute('aria-label', description ? `${label}. ${description}` : label)
  button.addEventListener('click', () => openBrowser(url))
  return button
}

// ─── Wishlist page ────────────────────────────────────────────────────────────

function renderWishlist () {
  const $ = id => document.getElementById(id)
  const loginPrompt = $('wishlist-login-prompt'), onSaleWrap = $('wishlist-on-sale')
  const loginActions = $('wishlist-signin-actions'), accountHint = $('wishlist-account-hint')
  const allWrap = $('wishlist-all-section'), emptyEl = $('empty-wishlist')
  const panel = $('wishlist-deal-panel'), tabs = $('wishlist-tabs'), errs = $('wishlist-errors')
  const controls = $('wishlist-controls'), search = $('wishlist-search'), sort = $('wishlist-sort')
  const saleOnly = $('wishlist-sale-only'), filters = state.listFilters.wishlist

  emptyEl.classList.toggle('loading-state', !state.dataLoaded)
  updateBadges()
  if ($('lc-wishlist')) $('lc-wishlist').textContent = fmtChecked(state.lastChecked)
  const hide = (...els) => els.forEach(e => { if (e) e.style.display = 'none' })

  if (!hasAccount()) {
    loginPrompt.style.display = 'flex'
    loginActions.replaceChildren(...Object.keys(STORES).map(store => {
      const button = document.createElement('button')
      button.type = 'button'; button.className = 'action-btn wishlist-signin-btn'; button.textContent = 'Sign in to ' + STORES[store]
      button.addEventListener('click', () => signInStore(store, button))
      return button
    }))
    accountHint.style.display = 'none'
    hide(onSaleWrap, allWrap, emptyEl, panel, tabs, errs, controls)
    return
  }
  hide(loginPrompt)
  const missingStores = Object.keys(STORES).filter(store => !state.accounts[store])
  accountHint.textContent = missingStores.length ? 'Sign in to ' + missingStores.map(store => STORES[store]).join(' and ') + ' to include their wishlists.' : ''
  accountHint.style.display = missingStores.length ? 'block' : 'none'

  // Per-store failures (e.g. Epic rejecting the token) shown instead of silently empty
  const failed = Object.entries(state.wishlistErrors).filter(([s]) => state.accounts[s])
  errs.classList.add('err')
  errs.style.display = failed.length ? 'flex' : 'none'
  errs.textContent = failed.map(([s, e]) => `${STORES[s]} wishlist: ${e}`).join('  ·  ')

  // Store filter tabs (only for signed-in stores)
  const stores = Object.keys(STORES).filter(s => state.accounts[s])
  if (!stores.includes(state.wishlistTab)) state.wishlistTab = 'all'
  tabs.style.display = stores.length > 1 ? 'flex' : 'none'
  renderTabList(tabs, ['all', ...stores].map(s => ({
    key: s, text: `${s === 'all' ? 'All' : STORES[s]} (${s === 'all' ? state.wishlist.length : state.wishlist.filter(w => w.store === s).length})`,
  })), state.wishlistTab, 'Wishlist stores', s => { state.wishlistTab = s; renderWishlist() })

  const items = state.wishlistTab === 'all' ? state.wishlist : state.wishlist.filter(w => w.store === state.wishlistTab)
  const visibleCount = filteredSorted(items, 'wishlist', {
    title: item => item.name, price: item => item.priceInfo?.final ?? Infinity, discount: item => item.priceInfo?.discount ?? 0,
  }).length
  updateResultCount('wishlist', visibleCount, items.length)
  emptyEl.querySelector('.clear-filter')?.remove()
  controls.style.display = items.length ? 'flex' : 'none'
  search.value = filters.query
  sort.value = filters.sort
  saleOnly.setAttribute('aria-pressed', String(filters.onSaleOnly))
  if (!items.length) {
    hide(onSaleWrap, allWrap, panel)
    if (!state.dataLoaded) {
      hide(onSaleWrap, panel, emptyEl)
      allWrap.style.display = 'block'
      renderSkeletons($('list-wishlist-all'), 'row')
      return
    }
    const steamOnly = state.wishlistTab === 'steam' || (state.wishlistTab === 'all' && state.accounts.steam && !state.accounts.gog && !state.accounts.epic)
    $('wishlist-empty-msg').textContent = steamOnly && state.wishlistPrivate
      ? 'Your Steam wishlist is set to Private. Make it Public in Steam → Privacy Settings.'
      : failed.length ? 'Wishlist could not be loaded. See “Something went wrong” in the sidebar, then choose Copy error.'
      : state.wishlist.length ? 'No games in this store wishlist.' : 'Wishlist is empty.'
    emptyEl.style.display = 'flex'
    return
  }
  const visibleItems = filteredSorted(items, 'wishlist', {
    title: item => item.name,
    price: item => item.priceInfo?.final ?? Infinity,
    discount: item => item.priceInfo?.discount ?? 0,
  })
  if (!visibleItems.length) {
    hide(onSaleWrap, allWrap, panel)
    $('wishlist-empty-msg').textContent = 'No wishlist games match.'
    showFilterEmpty(emptyEl, 'wishlist', 'wishlist-search', null, renderWishlist, 'No wishlist games match.', { saleOnlyId: 'wishlist-sale-only' })
    emptyEl.style.display = 'flex'
    return
  }
  emptyEl.style.display = 'none'

  const listSale = $('list-wishlist-sale'), listAll = $('list-wishlist-all')
  listSale.innerHTML = ''; listAll.innerHTML = ''
  const onSale = visibleItems.filter(w => (w.priceInfo?.discount ?? 0) >= 20)
  onSaleWrap.style.display = onSale.length ? 'block' : 'none'
  onSale.forEach(w => listSale.appendChild(makeWishlistSaleRow(w)))
  allWrap.style.display = 'block'
  visibleItems.forEach(w => listAll.appendChild(makeWishlistRow(w)))
  panel.style.display = state.selectedWishlistDeal && visibleItems.some(w => w.store === 'steam' && String(w.appId) === String(state.selectedWishlistDeal.appId)) ? 'block' : 'none'
  renderWishlistDealPanel()
}

const storeTag = s => `<span class="store-tag ${escHtml(s)}">${escHtml(STORES[s] ?? s)}</span>`

// Where "open" should go: the cheapest known offer if another store beats this one
const bestUrl = w => w.cheapest?.url ?? w.url

function renderTabList (container, choices, selected, label, onSelect) {
  container.setAttribute('role', 'tablist')
  container.setAttribute('aria-label', label)
  container.replaceChildren()
  const activate = key => {
    onSelect(key)
    ;[...container.querySelectorAll('[role="tab"]')].find(tab => tab.dataset.tabKey === key)?.focus()
  }
  choices.forEach(({key, text}) => {
    const tab = document.createElement('button')
    tab.type = 'button'; tab.className = 'tab' + (key === selected ? ' active' : ''); tab.setAttribute('role', 'tab')
    tab.dataset.tabKey = key; tab.textContent = text
    tab.setAttribute('aria-selected', String(key === selected)); tab.tabIndex = key === selected ? 0 : -1
    tab.addEventListener('click', () => activate(key))
    tab.addEventListener('keydown', e => {
      const index = choices.findIndex(choice => choice.key === key)
      let next = index
      if (e.key === 'ArrowRight') next = (index + 1) % choices.length
      else if (e.key === 'ArrowLeft') next = (index - 1 + choices.length) % choices.length
      else if (e.key === 'Home') next = 0
      else if (e.key === 'End') next = choices.length - 1
      else return
      e.preventDefault(); activate(choices[next].key)
    })
    container.appendChild(tab)
  })
}

function makeWishlistSaleRow (w) {
  const p   = w.priceInfo
  const row = document.createElement('div')
  row.className = 'deal-row'
  const main = document.createElement('button')
  main.type = 'button'
  main.className = 'deal-row-main wishlist-sale-main'
  main.innerHTML = `
    <div>
      <div class="deal-name">${escHtml(w.name)}</div>
      <div class="deal-store">${storeTag(w.store)}${w.cheapest ? `<span class="cheapest-tag">↘ ${escHtml(w.cheapest.store)} ${fmt$(w.cheapest.price)}</span>` : ''}</div>
    </div>
    <span class="sr-only">${escHtml(salePriceDescription(p.initial, p.final, p.discount, p.formatted))}</span>
    <span class="deal-orig" aria-hidden="true">${fmt$(p.initial)}</span>
    <span class="deal-price" aria-hidden="true">${escHtml(p.formatted ?? fmt$(p.final))}</span>
    <span class="deal-cut" aria-hidden="true">-${escHtml(p.discount)}%</span>`
  main.querySelector('.deal-name').setAttribute('title', w.name)
  const btn = tmpl(`<button class="deal-open" title="${w.cheapest ? 'Open cheapest offer' : 'Open store page'}">↗</button>`)
  btn.setAttribute('aria-label', `Open ${w.name} at ${w.cheapest?.store || gameStoreName(w.store)}`)
  btn.addEventListener('click', e => { e.stopPropagation(); openBrowser(bestUrl(w)) })
  main.addEventListener('click', () => openBrowser(bestUrl(w)))
  row.append(main, btn)
  return row
}

function makeWishlistRow (w) {
  const p      = w.priceInfo
  const onSale = (p?.discount ?? 0) >= 20
  const row    = document.createElement('div')
  row.className = 'wishlist-row' + (onSale ? ' on-sale' : '')
  const main = document.createElement('button')
  main.type = 'button'
  main.className = 'wishlist-main'

  const thumb = document.createElement('img')
  thumb.className = 'wishlist-thumb'
  thumb.src   = w.image ?? ''
  thumb.alt   = ''
  thumb.dataset.fb = ''
  thumb.style.visibility = w.image ? '' : 'hidden'

  const info  = document.createElement('div')
  const priceStr = p?.formatted
    ? (onSale
        ? `<span class="sr-only">${escHtml(salePriceDescription(p.initial, p.final, p.discount, p.formatted))}</span><span aria-hidden="true" style="color:var(--amber)">-${escHtml(p.discount)}% · ${escHtml(p.formatted)}</span>`
        : `<span style="color:var(--text2)">${escHtml(p.formatted)}</span>`)
    : ''
  const cheap = w.cheapest ? ` <span class="cheapest-tag">${ARROW_DOWN} cheaper at ${escHtml(w.cheapest.store)}: ${fmt$(w.cheapest.price)}</span>` : ''
  info.innerHTML = `<div class="wishlist-name">${storeTag(w.store)}${escHtml(w.name)}</div><div class="wishlist-sub">${priceStr}${cheap}</div>`
  info.querySelector('.wishlist-name').setAttribute('title', w.name)

  main.append(thumb, info)
  main.addEventListener('click', () => w.store === 'steam' ? selectWishlistGame(w) : openBrowser(bestUrl(w)))
  row.append(main, makeIconBtns(w))
  return row
}

function makeIconBtns (w) {
  const wrap     = document.createElement('div')
  wrap.className = 'wishlist-actions'
  const ch = launcherOf(w.store)
  const storeName = gameStoreName(w.store)
  if (ch) {
    const appBtn = tmpl(`<button class="icon-btn" title="Open in launcher">${LOGO_PATH[w.store] ? logo(w.store) : escHtml(ch.label[0])}</button>`)
    appBtn.title = `Open in ${ch.label}`
    appBtn.setAttribute('aria-label', `Open ${w.name} in ${ch.label}`)
    appBtn.addEventListener('click', e => { e.stopPropagation(); window.api.openInApp(w.store, w.url, w.id) })
    wrap.append(appBtn)
  }
  const webBtn = tmpl(`<button class="icon-btn" title="${w.cheapest ? `Open cheapest offer (${escHtml(w.cheapest.store)})` : 'Open in browser'}">${w.cheapest ? ARROW_DOWN : '↗'}</button>`)
  webBtn.setAttribute('aria-label', `Open ${w.name} at ${w.cheapest?.store || storeName} in browser`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(bestUrl(w)) })
  wrap.append(webBtn)
  return wrap
}

async function selectWishlistGame (w) {
  const empty = { loading: false, appId: w.appId, gameName: w.name, steam: null, cheapest: null, checkedAt: Date.now() }
  state.selectedWishlistDeal = { ...empty, loading: true }
  renderWishlist()
  try {
    state.selectedWishlistDeal = { loading: false, ...(await window.api.fetchWishlistGameDeals({ appId: w.appId, name: w.name })) }
  } catch (e) {
    console.error('[wishlist-deal-panel]', e)
    state.selectedWishlistDeal = empty
  }
  renderWishlist()
}

function renderWishlistDealPanel () {
  const panel = document.getElementById('wishlist-deal-panel')
  const d = state.selectedWishlistDeal
  if (!panel || !d) return

  const gameName = d.gameName ?? `App ${d.appId}`
  panel.querySelector('#wishlist-deal-title').textContent = gameName
  panel.querySelector('#wishlist-deal-updated').textContent = d.loading ? 'Loading…' : fmtChecked(d.checkedAt)

  const steamPrice = panel.querySelector('#wishlist-steam-price')
  const steamSub = panel.querySelector('#wishlist-steam-sub')
  if (d.loading) {
    steamPrice.textContent = 'Loading…'
    steamSub.textContent = 'Checking Steam price…'
  } else if (d.steam) {
    steamPrice.textContent = d.steam.formatted || fmt$(d.steam.final || 0)
    steamSub.textContent = d.steam.onSale
      ? `Steam sale live: ${d.steam.discount}% off (was ${fmt$(d.steam.initial || 0)}).`
      : 'No active Steam discount right now.'
  } else {
    steamPrice.textContent = '-'
    steamSub.textContent = 'Steam price unavailable for this game.'
  }

  const cheapestPrice = panel.querySelector('#wishlist-cheapest-price')
  const cheapestSub = panel.querySelector('#wishlist-cheapest-sub')
  const cheapestBtn = panel.querySelector('#wishlist-open-cheapest')
  cheapestBtn.setAttribute('aria-label', `Open ${gameName} at ${d.cheapest?.store ?? 'the cheapest store'}`)
  if (d.loading) {
    cheapestPrice.textContent = 'Loading…'
    cheapestSub.textContent = 'Searching external stores…'
    cheapestBtn.style.display = 'none'
  } else if (d.cheapest) {
    cheapestPrice.textContent = fmt$(d.cheapest.price || 0)
    cheapestSub.textContent = `${d.cheapest.store} (${d.cheapest.savingsPercent}% off, normal ${fmt$(d.cheapest.retailPrice || 0)}).`
    cheapestBtn.style.display = d.cheapest.url ? 'inline-flex' : 'none'
    cheapestBtn.onclick = () => d.cheapest.url && openBrowser(d.cheapest.url)
  } else {
    cheapestPrice.textContent = '-'
    cheapestSub.textContent = 'No third-party deals found for this game.'
    cheapestBtn.style.display = 'none'
    cheapestBtn.onclick = null
  }
  const steamBtn = panel.querySelector('#wishlist-open-steam')
  steamBtn.setAttribute('aria-label', `Open ${gameName} on Steam`)
  steamBtn.onclick = () => openSteam(d.appId)
}

// ─── Free games page ──────────────────────────────────────────────────────────

function renderFreeGames () {
  const gridFree  = document.getElementById('grid-free')
  const gridWknd  = document.getElementById('grid-weekends')
  const lblWknd   = document.getElementById('label-weekends')
  const emptyEl   = document.getElementById('empty-free')
  const tsEl      = document.getElementById('lc-free')

  emptyEl.classList.toggle('loading-state', !state.dataLoaded)
  updateBadges()
  gridFree.innerHTML = ''
  gridWknd.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.freeGames.length && !state.freeWeekends.length) {
    if (!state.dataLoaded) {
      renderSkeletons(gridFree, 'card')
      emptyEl.style.display = 'none'
      return
    }
    const mins = Number(state.settings.checkInterval) || 60
    document.getElementById('free-empty-copy').textContent = `No free games right now. We check every ${mins < 60 ? mins + ' minutes' : mins / 60 + ' hour' + (mins === 60 ? '' : 's')}.`
    emptyEl.style.display = 'flex'
    return
  }
  emptyEl.style.display = 'none'
  endsFirst(state.freeGames).forEach(g => gridFree.appendChild(makeFreeCard(g, 'tag-free', 'FREE')))

  if (state.freeWeekends.length) {
    lblWknd.style.display = 'block'
    endsFirst(state.freeWeekends).forEach(g => gridWknd.appendChild(makeFreeCard(g, 'tag-weekend', 'WEEKEND')))
  } else {
    lblWknd.style.display = 'none'
  }
}

// Sidebar badges — computed from state so they stay correct even when the
// owning page isn't the one being rendered right now
function updateBadges () {
  const onSale = onSaleItems().length
  const wb = document.getElementById('badge-wishlist')
  if (wb) { wb.querySelector('.badge-count').textContent = onSale; wb.classList.toggle('visible', onSale > 0) }

  const total = state.freeGames.length + state.freeWeekends.length
  const fb = document.getElementById('badge-free')
  if (fb) { fb.querySelector('.badge-count').textContent = total; fb.classList.toggle('visible', total > 0) }
}

function makeFreeCard (g, tagClass, tagText) {
  const card = document.createElement('div')
  card.className = 'game-card'
  const srcClass = ['steam','epic','gog'].includes(g.source) ? g.source : 'other'
  card.innerHTML = `
    <div class="game-card-content" aria-hidden="true">
    <div class="game-thumb">
      ${g.image
        ? `<img src="${escHtml(g.image)}" alt="" data-fb="${srcIcon(g.source)}">`
        : `<span class="fallback">${srcIcon(g.source)}</span>`}
      <span class="src-badge ${srcClass}">${escHtml(gameStoreName(g.source).toUpperCase())}</span>
    </div>
    <div class="game-info">
    <div class="game-title">${escHtml(g.title)}</div>
      <div class="price-row">
        <span class="${tagClass}">${tagText}</span>
        ${g.endDate ? `<span class="time-left">${escHtml(endDateLabel(g.endDate))}</span>` : ''}
      </div>
    </div></div>`
  card.appendChild(makeCardOpenButton(g.source, g.url, g.title))
  card.appendChild(makeCardActions(g.source, g.url, g.appId, g.title))
  card.querySelector('.game-title').setAttribute('title', g.title)
  return card
}

// ─── Deals page ───────────────────────────────────────────────────────────────

function renderDeals () {
  const list    = document.getElementById('list-deals')
  const notice  = document.getElementById('info-notice')
  const emptyEl = document.getElementById('empty-deals')
  const tsEl    = document.getElementById('lc-deals')
  document.getElementById('deals-sort').value = state.listFilters.deals.sort

  emptyEl.classList.toggle('loading-state', !state.dataLoaded)
  list.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  // Say why, instead of an unexplained empty list
  notice.classList.toggle('err', !!state.dealsError)
  notice.style.display = state.dealsError || state.dealsSource ? 'flex' : 'none'
  notice.innerHTML = state.dealsError
    ? `<div>${escHtml(state.dealsError)}</div>`
    : `Source: ${escHtml(state.dealsSource)}`

  const items = filteredSorted(state.deals, 'deals')
  syncMinDiscount('deals', 'deals-discount-filters')
  emptyEl.style.display = items.length ? 'none' : 'flex'
  if (!items.length && !state.dataLoaded) {
    renderSkeletons(list, 'row')
    emptyEl.style.display = 'none'
  } else if (!items.length && (state.listFilters.deals.query.trim() || state.listFilters.deals.minDiscount)) {
    showFilterEmpty(emptyEl, 'deals', 'deals-search', 'deals-discount-filters', renderDeals, 'No deals match.')
  } else if (!items.length) {
    emptyEl.replaceChildren(Object.assign(document.createElement('p'), { textContent: 'No deals could be loaded right now.' }))
  }
  items.forEach(d => list.appendChild(makeDealRow(d)))
  updateResultCount('deals', items.length, state.deals.length)
}

function makeDealRow (d) {
  const row = document.createElement('div')
  row.className = 'deal-row'
  const content = document.createElement('div')
  const shopClass = ['steam', 'gog', 'epic'].includes(storeKeyFor(d.shop)) ? storeKeyFor(d.shop) : 'other'
  content.className = 'deal-row-content'; content.setAttribute('aria-hidden', 'true')
  content.innerHTML = `<div class="deal-title-line"><div class="deal-name">${escHtml(d.title)}</div><span class="deal-shop-badge ${shopClass}">${escHtml(d.shop)}</span></div>
    <div class="deal-price-line"><span class="deal-orig">${fmt$(d.regular)}</span><span class="deal-price">${fmt$(d.price)}</span><span class="deal-cut">-${escHtml(d.cut)}%</span></div>`
  content.querySelector('.deal-name').setAttribute('title', d.title)
  const main = document.createElement('button')
  main.type = 'button'; main.className = 'deal-row-main'
  main.setAttribute('aria-label', `Open ${d.title} on ${d.shop}. ${salePriceDescription(d.regular, d.price, d.cut)}`)
  // Steam deals open in the Steam app when it's installed; everything else keeps CheapShark's redirect link.
  const open = () => d.appId && storeKeyFor(d.shop) === 'steam' && launcherOf('steam') ? openSteam(d.appId) : openBrowser(d.url)
  main.addEventListener('click', open)
  const btn = tmpl(`<button class="deal-open" title="Open store page">↗</button>`)
  btn.setAttribute('aria-label', `Open ${d.title} on ${d.shop}`)
  btn.addEventListener('click', e => { e.stopPropagation(); open() })
  row.append(content, main, btn)
  return row
}

// ─── Store sales page ─────────────────────────────────────────────────────────

function renderStoreSales () {
  const tabs = document.getElementById('sales-tabs')
  const grid = document.getElementById('grid-storesales')
  const emptyEl = document.getElementById('empty-storesales')
  document.getElementById('sales-sort').value = state.listFilters.storesales.sort
  document.getElementById('lc-storesales').textContent = fmtChecked(state.lastChecked)

  emptyEl.classList.toggle('loading-state', !state.dataLoaded)
  renderTabList(tabs, Object.keys(STORES).map(s => ({key:s, text:`${STORES[s]} (${state.sales[s]?.length ?? 0})`})),
    state.salesTab, 'Stores', s => { state.salesTab = s; renderStoreSales() })

  grid.innerHTML = ''
  const list = state.sales[state.salesTab] ?? []
  const items = filteredSorted(list, 'storesales')
  updateResultCount('storesales', items.length, list.length)
  syncMinDiscount('storesales', 'sales-discount-filters')
  emptyEl.style.display = items.length ? 'none' : 'flex'
  if (!items.length && !state.dataLoaded) {
    renderSkeletons(grid, 'card')
    emptyEl.style.display = 'none'
  } else if (!items.length && (state.listFilters.storesales.query.trim() || state.listFilters.storesales.minDiscount)) {
    showFilterEmpty(emptyEl, 'storesales', 'sales-search', 'sales-discount-filters', renderStoreSales, 'No store sales match.')
  } else if (!items.length) {
    emptyEl.replaceChildren(Object.assign(document.createElement('p'), { textContent: 'No discounts loaded for this store right now.' }))
  }
  items.forEach(d => grid.appendChild(makeSaleCard(d)))
}

function makeSaleCard (d) {
  const src = storeKeyFor(d.shop)
  const srcClass = ['steam','epic','gog'].includes(src) ? src : 'other'
  const card = document.createElement('div')
  card.className = 'game-card'
  card.innerHTML = `
    <div class="game-card-content" aria-hidden="true">
    <div class="game-thumb">
      ${d.image ? `<img src="${escHtml(d.image)}" alt="" data-fb="${srcIcon(src)}">` : `<span class="fallback">${srcIcon(src)}</span>`}
      <span class="src-badge ${srcClass}">${escHtml(gameStoreName(src).toUpperCase())}</span>
    </div>
    <div class="game-info">
      <div class="game-title">${escHtml(d.title)}</div>
      <div class="price-row">
        <span class="sr-only">${escHtml(salePriceDescription(d.regular, d.price, d.cut))}</span>
        <span class="price-was" aria-hidden="true">${fmt$(d.regular)}</span>
        <span class="price-now" aria-hidden="true">${fmt$(d.price)}</span>
        <span class="tag-sale" aria-hidden="true">-${escHtml(d.cut)}%</span>
      </div>
    </div></div>`
  card.appendChild(makeCardOpenButton(src, d.url, d.title, salePriceDescription(d.regular, d.price, d.cut)))
  card.appendChild(makeCardActions(src, d.url, d.appId ?? d.id, d.title))
  card.querySelector('.game-title').setAttribute('title', d.title)
  return card
}

// ─── Sales calendar ───────────────────────────────────────────────────────────

const saleKey = s => ['lunar', 'spring', 'summer', 'halloween', 'autumn', 'winter'].find(k => s.name.toLowerCase().includes(k))

// Live sale → its real banner page; otherwise Steam's Specials. Opens the Steam app if installed.
function openSale (s) {
  const live = state.liveSale
  const url = live && live.key === saleKey(s) && live.url ? live.url : 'https://store.steampowered.com/specials'
  window.api.openInApp('steam', url)
}

function renderSalesCalendar () {
  const banner   = document.getElementById('next-sale-banner')
  const timeline = document.getElementById('sale-timeline')
  const now      = new Date()
  timeline.innerHTML = ''

  const next = SALES.find(s => new Date(s.start) > now)
  if (next) {
    const days = Math.ceil((new Date(next.start) - now) / 86400000)
    banner.style.cursor = 'pointer'
    banner.setAttribute('role', 'button')
    banner.tabIndex = 0
    banner.dataset.keyboardAction = 'activate'
    banner.dataset.openSaleIndex = String(SALES.indexOf(next))
    banner.innerHTML = `
      <div class="nsb-icon">${next.icon}</div>
      <div class="nsb-info">
        <div class="nsb-title">${next.name}</div>
        <div class="nsb-sub">${next.est ? 'Estimated — based on past years' : 'Confirmed date'}</div>
      </div>
      <div class="nsb-countdown">
        <div class="nsb-val">${days}</div>
        <div class="nsb-lbl">${days === 1 ? 'day' : 'days'} away</div>
      </div>`
    banner.querySelector('.nsb-title').setAttribute('title', next.name)
  } else {
    banner.style.cursor = ''
    banner.removeAttribute('role')
    banner.removeAttribute('tabindex')
    delete banner.dataset.keyboardAction
    delete banner.dataset.openSaleIndex
    banner.innerHTML = `<div class="nsb-icon">✅</div><div class="nsb-info"><div class="nsb-title">No upcoming sales found</div></div>`
  }

  const fmtDate = d => d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' })
  SALES.forEach(s => {
    const start  = new Date(s.start)
    const end    = new Date(s.end)
    const isPast = end < now
    const isLive = start <= now && end >= now
    const isNext = s === next
    const item   = document.createElement('div')
    item.className = 'timeline-item clickable' + (isPast ? ' past' : '') + (isNext ? ' next' : '')
    item.setAttribute('role', 'button')
    item.tabIndex = 0
    item.dataset.keyboardAction = 'activate'
    item.dataset.openSaleIndex = String(SALES.indexOf(s))
    item.setAttribute('title', `Open ${s.name} on Steam`)
    item.innerHTML = `
      <div class="tl-dot"></div>
      <span class="tl-name">
        ${s.icon} ${s.name}
        ${isLive ? '<span style="color:var(--green);font-size:11px;margin-left:4px">(live now!)</span>' : ''}
        ${s.est  ? '<span style="font-size:10px;color:var(--text2);margin-left:4px">(est.)</span>'      : ''}
      </span>
      <span class="tl-date">${fmtDate(start)} – ${fmtDate(end)}</span>`
    item.querySelector('.tl-name').setAttribute('title', s.name)
    timeline.appendChild(item)
  })
}

// ─── Settings page ────────────────────────────────────────────────────────────

// ─── Themes ───────────────────────────────────────────────────────────────────
// Base themes are always available; a sale theme is only offered (and auto-applied)
// while that Steam sale is live.

const BASE_THEMES = {
  midnight:   { name: 'Midnight',   sw: ['#0e1117', '#66c0f4'] },
  lavender:   { name: 'Lavender',   sw: ['#13101f', '#c4a1ff', '#ff9ed2'] },
  spooktober: { name: 'Spooktober', sw: ['#0a0a0a', '#ff7a1a'] },
  forest:     { name: 'Forest',     sw: ['#0b1310', '#5fd38d'] },
  graphite:   { name: 'Graphite',   sw: ['#0f1011', '#e6e8eb'] },
  daylight:   { name: 'Daylight',   sw: ['#f2f4f8', '#1f73d1'] },
}
const SALE_THEMES = {
  lunar:     { name: 'Lunar New Year', sw: ['#1a0808', '#f7c948'], fx: ['🏮', '🧧', '✨'] },
  spring:    { name: 'Spring Sale',    sw: ['#190f16', '#ff9ec7'], fx: ['🌸', '🌷'] },
  summer:    { name: 'Summer Sale',    sw: ['#06161b', '#ffc83d'], fx: ['○', '◌', '☀'], rise: true },
  halloween: { name: 'Halloween Sale', sw: ['#0c0812', '#ff8a1f'], fx: ['🦇', '🎃', '👻'] },
  autumn:    { name: 'Autumn Sale',    sw: ['#150f0a', '#e8843a'], fx: ['🍂', '🍁', '🍃'] },
  winter:    { name: 'Winter Sale',    sw: ['#08121c', '#9bd7ff'], fx: ['❄', '❅', '❆'] },
}

function applyTheme () {
  const s = state.settings
  const root = document.documentElement
  root.dataset.theme = s.theme
  root.dataset.round = s.rounded ? 'on' : 'off'
  const fxKey = s.theme + s.effects
  if (fxKey !== applyTheme.fxKey) { applyTheme.fxKey = fxKey; startFx(SALE_THEMES[s.theme]) }
}

const LOOK_TOGGLES = [
  { key: 'rounded', label: 'Rounded window', sub: 'Round the corners of the app window' },
  { key: 'effects', label: 'Seasonal effects', sub: 'Falling leaves, snow & more during Steam sale themes' },
]
const NOTIFICATION_TOGGLES = [
  { key: 'notifyMajorSale', label: 'Steam major sale started', sub: 'Summer, Winter, Autumn & Spring sales' },
  { key: 'notifyFreeGame', label: 'Free game available', sub: 'Epic, Steam, GOG & GamerPower giveaways', gamerpower: true },
  { key: 'notifyFreeWeekend', label: 'Free weekend started', sub: 'Temporary Steam free-to-play promotions' },
  { key: 'notifyWishlistSale', label: 'Wishlist game on sale', sub: 'Steam, GOG & Epic wishlist games at 20% or more off' },
  { key: 'notifyBigDeal', label: 'Deal over 75% off', sub: 'Hot deals list (CheapShark)' },
]
window.dealDropOnboarding = { state, baseThemes: BASE_THEMES, lookToggles: LOOK_TOGGLES, notificationToggles: NOTIFICATION_TOGGLES, applyTheme, setAccounts, loadData, renderSettings, closeShortcutsDialog }

// A sale starting switches to its theme once. If the user picks something else, lastSaleTheme
// stays set, so a restart doesn't switch back. When the sale ends its theme is dropped.
function syncSaleTheme () {
  const s = state.settings
  const key = state.liveSale?.key ?? ''
  let dirty = false
  if (key && SALE_THEMES[key]) {
    if (s.lastSaleTheme !== key) {
      s.lastSaleTheme = key
      if (!state.onboardingOpen || !state.onboardingThemePicked) s.theme = key
      dirty = true
    }
  } else {
    if (SALE_THEMES[s.theme]) { s.theme = BASE_THEMES[s.baseTheme] ? s.baseTheme : 'midnight'; dirty = true }
    if (s.lastSaleTheme) { s.lastSaleTheme = ''; dirty = true }
  }
  if (dirty) window.api.saveSettings(s)
  applyTheme()
}

// Falling-glyph effect on a click-through canvas; off for reduced-motion or when disabled
let fxFrame = 0
function startFx (cfg) {
  cancelAnimationFrame(fxFrame)
  const cv = document.getElementById('fx')
  const ctx = cv.getContext('2d')
  const dpr = window.devicePixelRatio || 1
  cv.width = innerWidth * dpr; cv.height = innerHeight * dpr
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, innerWidth, innerHeight)
  if (!cfg || !state.settings.effects || matchMedia('(prefers-reduced-motion: reduce)').matches) return

  const dir = cfg.rise ? -1 : 1
  const parts = Array.from({ length: 34 }, () => ({
    x: Math.random() * innerWidth, y: Math.random() * innerHeight,
    g: cfg.fx[Math.floor(Math.random() * cfg.fx.length)],
    size: 12 + Math.random() * 14, v: 0.25 + Math.random() * 0.7,
    ph: Math.random() * 6.28, sway: 0.2 + Math.random() * 0.6, rot: (Math.random() - .5) * 0.02, a: 0,
  }))
  ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--acc')
  const tick = () => {
    ctx.clearRect(0, 0, innerWidth, innerHeight)
    for (const p of parts) {
      p.y += p.v * dir; p.ph += 0.015; p.a += p.rot
      p.x += Math.sin(p.ph) * p.sway
      if (dir > 0 && p.y > innerHeight + 30) { p.y = -30; p.x = Math.random() * innerWidth }
      if (dir < 0 && p.y < -30) { p.y = innerHeight + 30; p.x = Math.random() * innerWidth }
      ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(p.a)
      ctx.globalAlpha = 0.7; ctx.font = `${p.size}px sans-serif`; ctx.fillText(p.g, 0, 0)
      ctx.restore()
    }
    fxFrame = requestAnimationFrame(tick)
  }
  tick()
}
addEventListener('resize', () => startFx(SALE_THEMES[state.settings.theme]))

function swatch (cols) { return `<span class="swatch">${cols.map(c => `<i style="background:${c}"></i>`).join('')}</span>` }

function renderThemeGrid () {
  const s = state.settings
  const grid = document.getElementById('theme-grid')
  grid.innerHTML = ''
  const entries = Object.entries(BASE_THEMES).map(([k, v]) => [k, v, false])
  const live = state.liveSale?.key
  if (live && SALE_THEMES[live]) entries.push([live, SALE_THEMES[live], true])
  entries.forEach(([key, th, isSale]) => {
    const b = document.createElement('button')
    b.className = 'theme-chip' + (s.theme === key ? ' active' : '') + (isSale ? ' sale' : '')
    b.innerHTML = `${swatch(th.sw)}${th.name}${isSale ? ' · live' : ''}`
    b.addEventListener('click', () => {
      s.theme = key
      if (!isSale) s.baseTheme = key
      window.api.saveSettings(s)
      applyTheme(); renderThemeGrid()
    })
    grid.appendChild(b)
  })
}

// ─── Settings page ────────────────────────────────────────────────────────────

function toggleRows (con, rows, after) {
  const s = state.settings
  con.innerHTML = ''
  rows.forEach(r => {
    const row = document.createElement('div')
    row.className = 'setting-row'
    const copy = document.createElement('div')
    const label = document.createElement('div'); label.className = 'setting-label'; label.textContent = r.label
    const sub = document.createElement('div'); sub.className = 'setting-sub'
    if (r.gamerpower) {
      sub.appendChild(document.createTextNode('Epic, Steam, GOG & '))
      const link = document.createElement('a'); link.href = '#'; link.dataset.href = 'https://www.gamerpower.com'; link.textContent = 'GamerPower'
      sub.append(link, document.createTextNode(' giveaways'))
    } else sub.textContent = r.sub
    copy.append(label, sub)
    const toggle = document.createElement('button')
    toggle.type = 'button'; toggle.className = 'toggle' + (s[r.key] ? ' on' : ''); toggle.dataset.key = r.key
    toggle.setAttribute('aria-label', r.label)
    toggle.setAttribute('aria-pressed', String(Boolean(s[r.key])))
    toggle.addEventListener('click', e => {
      const btn = e.currentTarget
      btn.classList.toggle('on')
      s[r.key] = btn.classList.contains('on')
      btn.setAttribute('aria-pressed', String(s[r.key]))
      window.api.saveSettings(s)
      after?.()
    })
    row.append(copy, toggle)
    con.appendChild(row)
  })
}

// "Open GOG/Epic games in": Auto prefers the store's own client, then Heroic, then Lutris
function renderLauncherPrefs () {
  const con = document.getElementById('launcher-prefs')
  con.innerHTML = ''
  ;[['gog', 'launcherGog', 'GOG'], ['epic', 'launcherEpic', 'Epic Games']].forEach(([store, key, name]) => {
    const shortName = name === 'Epic Games' ? 'Epic' : name
    const row = document.createElement('div')
    row.className = 'setting-row launcher-pref-row'
    const copy = document.createElement('div')
    const label = document.createElement('div'); label.className = 'setting-label'; label.textContent = `Open ${shortName} games in`
    const sub = document.createElement('div'); sub.className = 'setting-sub'
    const detected = state.launchers[store]?.options ?? []
    sub.textContent = detected.length ? 'Detected: ' + detected.map(o => o.label).join(', ') : 'No launcher detected — opens in the browser'
    copy.append(label, sub)
    const grid = document.createElement('div'); grid.className = 'theme-grid launcher-pref-grid'
    row.append(copy, grid)
    const choices = [{ key: 'auto', label: 'Auto' }, ...(state.launchers[store]?.options ?? []), { key: 'browser', label: 'Browser' }]
    choices.forEach(c => {
      const b = document.createElement('button')
      b.className = 'theme-chip' + (state.settings[key] === c.key ? ' active' : '')
      b.textContent = c.label
      b.addEventListener('click', async () => {
        state.settings[key] = c.key
        await window.api.saveSettings(state.settings)
        state.launchers = await window.api.launchers()
        renderLauncherPrefs()
      })
      grid.appendChild(b)
    })
    con.appendChild(row)
  })
}

function renderSettings () {
  const s = state.settings
  renderThemeGrid()
  toggleRows(document.getElementById('look-settings'), LOOK_TOGGLES, applyTheme)

  toggleRows(document.getElementById('notif-settings'), NOTIFICATION_TOGGLES)

  const ig = document.getElementById('interval-group')
  ig.innerHTML = ''
  ;[15, 60, 240, 720].forEach(mins => {
    const b = document.createElement('button')
    b.className   = 'interval-btn' + (s.checkInterval === mins ? ' active' : '')
    b.textContent = mins < 60 ? `${mins} min` : `${mins / 60} hr`
    b.addEventListener('click', () => {
      ig.querySelectorAll('.interval-btn').forEach(x => x.classList.remove('active'))
      b.classList.add('active')
      s.checkInterval = mins
      window.api.saveSettings(s)
    })
    ig.appendChild(b)
  })
  renderLauncherPrefs()
  renderSettingsAccounts()
  renderAbout()
  const reset = document.getElementById('btn-reset-settings')
  reset.textContent = resetSettingsArmed ? 'Really reset?' : 'Reset settings to defaults'
  reset.setAttribute('aria-label', resetSettingsArmed ? 'Confirm reset settings to defaults' : 'Reset settings to defaults')
}

function renderSettingsAccounts () {
  const con = document.getElementById('settings-accounts')
  con.replaceChildren()
  Object.keys(STORES).forEach(store => {
    const row = document.createElement('div'); row.className = 'setting-row settings-account-row'
    const account = state.accounts[store]
    const status = document.createElement('span'); status.className = 'setting-label'
    const name = gameStoreName(store)
    status.textContent = account ? `${name} — signed in as ${account.name || name + ' user'}` : `${name} — not signed in`
    row.appendChild(status)
    if (account) {
      const signOut = document.createElement('button'); signOut.type = 'button'; signOut.className = 'acct-out'; signOut.textContent = 'Sign out'
      signOut.addEventListener('click', async () => {
        if (signOut.disabled) return
        signOut.disabled = true
        try {
          setAccounts(await window.api.logout(store))
          state.wishlist = state.wishlist.filter(item => item.store !== store)
          updateBadges()
          renderErrors()
          navigate(state.page)
        } finally { signOut.disabled = false }
      })
      row.appendChild(signOut)
    }
    con.appendChild(row)
  })
}

function renderAbout () {
  const projectLinks = document.getElementById('about-project-links')
  const sources = document.getElementById('about-sources')
  projectLinks.replaceChildren()
  ;[['GitHub repository', REPO], ['Report an issue', REPO + '/issues']].forEach(([label, url], i) => {
    if (i) projectLinks.appendChild(document.createTextNode(' · '))
    const link = document.createElement('a'); link.href = '#'; link.dataset.href = url; link.textContent = label; projectLinks.appendChild(link)
  })
  sources.replaceChildren()
  const credits = [
    ['Steam', 'https://store.steampowered.com/'], ['GOG', 'https://www.gog.com/'],
    ['Epic Games', 'https://store.epicgames.com/'], ['GamerPower', 'https://www.gamerpower.com'],
    ['CheapShark', 'https://www.cheapshark.com'],
  ]
  credits.forEach(([name, url]) => {
    const item = document.createElement('li'), link = document.createElement('a')
    link.href = '#'; link.dataset.href = url; link.textContent = name; item.appendChild(link); sources.appendChild(item)
  })
  const fonts = document.createElement('li'); fonts.textContent = 'DM Sans and DM Mono — SIL Open Font License (OFL)'; sources.appendChild(fonts)
}

let resetSettingsArmed = false
let resetSettingsTimer = null
function clearResetSettingsConfirmation () {
  clearTimeout(resetSettingsTimer)
  resetSettingsTimer = null
  if (!resetSettingsArmed) return
  resetSettingsArmed = false
  const button = document.getElementById('btn-reset-settings')
  if (button) {
    button.textContent = 'Reset settings to defaults'
    button.setAttribute('aria-label', 'Reset settings to defaults')
  }
}
function resetSettings () {
  const settings = state.settings
  Object.assign(settings, DEFAULT_SETTINGS, {
    launcherGog: settings.launcherGog, launcherEpic: settings.launcherEpic, onboarded: settings.onboarded,
    lastSaleTheme: SALE_THEMES[state.liveSale?.key] ? state.liveSale.key : '',
  })
  state.listFilters = {
    deals: { query: '', sort: 'discount', minDiscount: 0 }, storesales: { query: '', sort: 'discount', minDiscount: 0 },
    wishlist: { query: '', sort: 'discount', onSaleOnly: false },
  }
  clearUiPrefs()
}

// ─── Navigation ───────────────────────────────────────────────────────────────

function navigate (page, persist = true) {
  if (state.page === 'settings' && page !== 'settings') clearResetSettingsConfirmation()
  const changed = state.page !== page
  state.page = page
  if (persist) persistUiPrefs()
  if (changed) {
    const main = document.querySelector('.main-content')
    if (main) main.scrollTo({ top: 0, behavior: 'auto' })
  }
  document.querySelectorAll('.nav-item').forEach(el => {
    const active = el.dataset.page === page
    el.classList.toggle('active', active)
    if (active) el.setAttribute('aria-current', 'page')
    else el.removeAttribute('aria-current')
  })
  document.querySelectorAll('.page').forEach(el => {
    const active = el.id === 'page-' + page
    el.classList.toggle('active', active)
    if (active && changed) {
      el.classList.remove('page-enter')
      void el.offsetWidth
      el.classList.add('page-enter')
    }
  })
  switch (page) {
    case 'wishlist':  renderWishlist();      break
    case 'free':      renderFreeGames();     break
    case 'storesales': renderStoreSales();   break
    case 'deals':     renderDeals();         break
    case 'sales':     renderSalesCalendar(); break
    case 'settings':  renderSettings();      break
  }
}

// ─── Store accounts (sidebar) ─────────────────────────────────────────────────

const ACCT_ICON = { steam: 'ST', gog: 'GOG', epic: 'EGS' } // text fallback if an avatar image fails to load
// Store logos (simple-icons, CC0)
const LOGO_PATH = {
  steam: 'M11.979 0C5.678 0 .511 4.86.022 11.037l6.432 2.658c.545-.371 1.203-.59 1.912-.59.063 0 .125.004.188.006l2.861-4.142V8.91c0-2.495 2.028-4.524 4.524-4.524 2.494 0 4.524 2.031 4.524 4.527s-2.03 4.525-4.524 4.525h-.105l-4.076 2.911c0 .052.004.105.004.159 0 1.875-1.515 3.396-3.39 3.396-1.635 0-3.016-1.173-3.331-2.727L.436 15.27C1.862 20.307 6.486 24 11.979 24c6.627 0 11.999-5.373 11.999-12S18.605 0 11.979 0zM7.54 18.21l-1.473-.61c.262.543.714.999 1.314 1.25 1.297.539 2.793-.076 3.332-1.375.263-.63.264-1.319.005-1.949s-.75-1.121-1.377-1.383c-.624-.26-1.29-.249-1.878-.03l1.523.63c.956.4 1.409 1.5 1.009 2.455-.397.957-1.497 1.41-2.454 1.012H7.54zm11.415-9.303c0-1.662-1.353-3.015-3.015-3.015-1.665 0-3.015 1.353-3.015 3.015 0 1.665 1.35 3.015 3.015 3.015 1.663 0 3.015-1.35 3.015-3.015zm-5.273-.005c0-1.252 1.013-2.266 2.265-2.266 1.249 0 2.266 1.014 2.266 2.266 0 1.251-1.017 2.265-2.266 2.265-1.253 0-2.265-1.014-2.265-2.265z',
  gog:   'M7.15 15.24H4.36a.4.4 0 0 0-.4.4v2c0 .21.18.4.4.4h2.8v1.32h-3.5c-.56 0-1.02-.46-1.02-1.03v-3.39c0-.56.46-1.02 1.03-1.02h3.48v1.32zM8.16 11.54c0 .58-.47 1.05-1.05 1.05H2.63v-1.35h3.78a.4.4 0 0 0 .4-.4V6.39a.4.4 0 0 0-.4-.4H4.39a.4.4 0 0 0-.41.4v2.02c0 .23.18.4.4.4H6v1.35H3.68c-.58 0-1.05-.46-1.05-1.04V5.68c0-.57.47-1.04 1.05-1.04H7.1c.58 0 1.05.47 1.05 1.04v5.86zM21.36 19.36h-1.32v-4.12h-.93a.4.4 0 0 0-.4.4v3.72h-1.33v-4.12h-.93a.4.4 0 0 0-.4.4v3.72h-1.33v-4.42c0-.56.46-1.02 1.03-1.02h5.61v5.44zM21.37 11.54c0 .58-.47 1.05-1.05 1.05h-4.48v-1.35h3.78a.4.4 0 0 0 .4-.4V6.39a.4.4 0 0 0-.4-.4h-2.03a.4.4 0 0 0-.4.4v2.02c0 .23.18.4.4.4h1.62v1.35H16.9c-.58 0-1.05-.46-1.05-1.04V5.68c0-.57.47-1.04 1.05-1.04h3.43c.58 0 1.05.47 1.05 1.04v5.86zM13.72 4.64h-3.44c-.58 0-1.04.47-1.04 1.04v3.44c0 .58.46 1.04 1.04 1.04h3.44c.57 0 1.04-.46 1.04-1.04V5.68c0-.57-.47-1.04-1.04-1.04m-.3 1.75v2.02a.4.4 0 0 1-.4.4h-2.03a.4.4 0 0 1-.4-.4V6.4c0-.22.17-.4.4-.4H13c.23 0 .4.18.4.4zM12.63 13.92H9.24c-.57 0-1.03.46-1.03 1.02v3.39c0 .57.46 1.03 1.03 1.03h3.39c.57 0 1.03-.46 1.03-1.03v-3.39c0-.56-.46-1.02-1.03-1.02m-.3 1.72v2a.4.4 0 0 1-.4.4v-.01H9.94a.4.4 0 0 1-.4-.4v-1.99c0-.22.18-.4.4-.4h2c.22 0 .4.18.4.4zM23.49 1.1a1.74 1.74 0 0 0-1.24-.52H1.75A1.74 1.74 0 0 0 0 2.33v19.34a1.74 1.74 0 0 0 1.75 1.75h20.5A1.74 1.74 0 0 0 24 21.67V2.33c0-.48-.2-.92-.51-1.24m0 20.58a1.23 1.23 0 0 1-1.24 1.24H1.75A1.23 1.23 0 0 1 .5 21.67V2.33a1.23 1.23 0 0 1 1.24-1.24h20.5a1.24 1.24 0 0 1 1.24 1.24v19.34z',
  epic:  'M3.537 0C2.165 0 1.66.506 1.66 1.879V18.44a4.262 4.262 0 00.02.433c.031.3.037.59.316.92.027.033.311.245.311.245.153.075.258.13.43.2l8.335 3.491c.433.199.614.276.928.27h.002c.314.006.495-.071.928-.27l8.335-3.492c.172-.07.277-.124.43-.2 0 0 .284-.211.311-.243.28-.33.285-.621.316-.92a4.261 4.261 0 00.02-.434V1.879c0-1.373-.506-1.88-1.878-1.88zm13.366 3.11h.68c1.138 0 1.688.553 1.688 1.696v1.88h-1.374v-1.8c0-.369-.17-.54-.523-.54h-.235c-.367 0-.537.17-.537.539v5.81c0 .369.17.54.537.54h.262c.353 0 .523-.171.523-.54V8.619h1.373v2.143c0 1.144-.562 1.71-1.7 1.71h-.694c-1.138 0-1.7-.566-1.7-1.71V4.82c0-1.144.562-1.709 1.7-1.709zm-12.186.08h3.114v1.274H6.117v2.603h1.648v1.275H6.117v2.774h1.74v1.275h-3.14zm3.816 0h2.198c1.138 0 1.7.564 1.7 1.708v2.445c0 1.144-.562 1.71-1.7 1.71h-.799v3.338h-1.4zm4.53 0h1.4v9.201h-1.4zm-3.13 1.235v3.392h.575c.354 0 .523-.171.523-.54V4.965c0-.368-.17-.54-.523-.54zm-3.74 10.147a1.708 1.708 0 01.591.108 1.745 1.745 0 01.49.299l-.452.546a1.247 1.247 0 00-.308-.195.91.91 0 00-.363-.068.658.658 0 00-.28.06.703.703 0 00-.224.163.783.783 0 00-.151.243.799.799 0 00-.056.299v.008a.852.852 0 00.056.31.7.7 0 00.157.245.736.736 0 00.238.16.774.774 0 00.303.058.79.79 0 00.445-.116v-.339h-.548v-.565H7.37v1.255a2.019 2.019 0 01-.524.307 1.789 1.789 0 01-.683.123 1.642 1.642 0 01-.602-.107 1.46 1.46 0 01-.478-.3 1.371 1.371 0 01-.318-.455 1.438 1.438 0 01-.115-.58v-.008a1.426 1.426 0 01.113-.57 1.449 1.449 0 01.312-.46 1.418 1.418 0 01.474-.309 1.58 1.58 0 01.598-.111 1.708 1.708 0 01.045 0zm11.963.008a2.006 2.006 0 01.612.094 1.61 1.61 0 01.507.277l-.386.546a1.562 1.562 0 00-.39-.205 1.178 1.178 0 00-.388-.07.347.347 0 00-.208.052.154.154 0 00-.07.127v.008a.158.158 0 00.022.084.198.198 0 00.076.066.831.831 0 00.147.06c.062.02.14.04.236.061a3.389 3.389 0 01.43.122 1.292 1.292 0 01.328.17.678.678 0 01.207.24.739.739 0 01.071.337v.008a.865.865 0 01-.081.382.82.82 0 01-.229.285 1.032 1.032 0 01-.353.18 1.606 1.606 0 01-.46.061 2.16 2.16 0 01-.71-.116 1.718 1.718 0 01-.593-.346l.43-.514c.277.223.578.335.9.335a.457.457 0 00.236-.05.157.157 0 00.082-.142v-.008a.15.15 0 00-.02-.077.204.204 0 00-.073-.066.753.753 0 00-.143-.062 2.45 2.45 0 00-.233-.062 5.036 5.036 0 01-.413-.113 1.26 1.26 0 01-.331-.16.72.72 0 01-.222-.243.73.73 0 01-.082-.36v-.008a.863.863 0 01.074-.359.794.794 0 01.214-.283 1.007 1.007 0 01.34-.185 1.423 1.423 0 01.448-.066 2.006 2.006 0 01.025 0zm-9.358.025h.742l1.183 2.81h-.825l-.203-.499H8.623l-.198.498h-.81zm2.197.02h.814l.663 1.08.663-1.08h.814v2.79h-.766v-1.602l-.711 1.091h-.016l-.707-1.083v1.593h-.754zm3.469 0h2.235v.658h-1.473v.422h1.334v.61h-1.334v.442h1.493v.658h-2.255zm-5.3.897l-.315.793h.624zm-1.145 5.19h8.014l-4.09 1.348z',
}
const logo = (s, px = 14) => `<svg class="logo" width="${px}" height="${px}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="${LOGO_PATH[s]}"/></svg>`
// "cheaper elsewhere": an arrow that bends down
const ARROW_DOWN = '<svg class="arrow-down" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 10 20 15 15 20"/><path d="M4 4v7a4 4 0 0 0 4 4h12"/></svg>'

function renderAccounts () {
  const con = document.getElementById('accounts')
  con.innerHTML = ''
  Object.keys(STORES).forEach(s => {
    const a = state.accounts[s]
    const name = a?.name
    const row = document.createElement('div')
    row.className = 'acct'
    const ico = a?.avatar ? `<img src="${escHtml(a.avatar)}" alt="" data-fb="${ACCT_ICON[s]}">` : logo(s, 15)
    row.innerHTML = `<span class="acct-ico ${s}">${ico}</span>` + (a
      ? `<div class="acct-main"><button class="acct-name" title="Open your ${STORES[s]} profile">${escHtml(name || STORES[s] + ' user')}</button><div class="acct-sub"><span class="store-tag ${s}">${STORES[s]}</span><button class="acct-out">Sign out</button></div></div>`
      : `<span class="acct-label">${STORES[s]}</span><button class="acct-btn">Sign in</button>`)
    const openProfile = () => a?.profileUrl && window.api.openInApp(s, a.profileUrl)
    row.querySelector('.acct-name')?.addEventListener('click', openProfile)
    row.querySelector('.acct-ico')?.addEventListener('click', openProfile)
    row.querySelector('.acct-out')?.addEventListener('click', async () => {
      setAccounts(await window.api.logout(s))
      state.wishlist = state.wishlist.filter(w => w.store !== s)
      navigate(state.page)
    })
    const inBtn = row.querySelector('.acct-btn')
    inBtn?.addEventListener('click', () => signInStore(s, inBtn))
    con.appendChild(row)
  })
}

async function signInStore (store, button) {
  button.disabled = true; button.textContent = 'Opening…'
  try {
    setAccounts(await window.api.login(store))
    await loadData()
  } catch (e) {
    if (String(e) !== 'Cancelled') alert(`${STORES[store]} sign-in failed: ${e}`)
  } finally {
    if (button.isConnected) { button.disabled = false; button.textContent = button.classList.contains('wishlist-signin-btn') ? 'Sign in to ' + STORES[store] : 'Sign in' }
  }
}

function setAccounts (acc) {
  state.accounts = { steam: acc?.steam ?? null, gog: acc?.gog ?? null, epic: acc?.epic ?? null }
  renderAccounts()
  if (state.page === 'wishlist') renderWishlist()
  if (state.page === 'settings') renderSettingsAccounts()
}

// ─── Data loading ─────────────────────────────────────────────────────────────

async function loadData (force = false) {
  if (state.refreshing) return
  state.refreshing = true
  if (force) announceStatus('Refreshing…')
  const buttons = [[document.getElementById('btn-check-now'), 'Refresh all'], [document.getElementById('btn-check-settings'), 'Refresh all']]
  buttons.forEach(([btn]) => { if (btn) { btn.disabled = true; btn.setAttribute('aria-busy', 'true') } })
  buttons.forEach(([btn]) => { if (btn) btn.textContent = 'Refreshing…' })
  try {
    if (force) state.launchers = await window.api.launchers().catch(() => state.launchers)
    state.fetchError = null
    applyData(await (force ? window.api.checkNow() : window.api.fetchData()))
    if (force) announceStatus('Updated')
  } catch (e) {
    console.error('[fetchData]', e)
    state.fetchError = `Couldn't update: ${String(e).slice(0, 90)}`
    renderErrors()
    if (force) announceStatus('Update failed')
    if (!state.dataLoaded) { state.dataLoaded = true; navigate(state.page) }
  } finally {
    state.refreshing = false
    buttons.forEach(([btn, label]) => { if (btn) { btn.disabled = false; btn.textContent = label; btn.removeAttribute('aria-busy') } })
  }
}

// Red box below accounts: only shown when something is wrong. It says so without details;
// "Copy error" puts the details on the clipboard for a bug report.
let lastErrorText = ''
function renderErrors () {
  const box = document.getElementById('sidebar-errors')
  if (!box) return
  const lines = [
    state.fetchError,
    ...state.sourceErrors,
    state.dealsError && `Hot deals: ${state.dealsError}`,
    ...Object.entries(state.wishlistErrors).filter(([s]) => state.accounts[s]).map(([s, e]) => `${STORES[s] ?? s} wishlist: ${e}`),
  ].filter(Boolean)
  lastErrorText = lines.length ? `DealDrop ${document.getElementById('app-version')?.textContent.replace('DealDrop ', '') ?? ''}\n${lines.join('\n')}` : ''
  box.style.display = lines.length ? 'block' : 'none'
  if (!lines.length) { box.innerHTML = ''; return }
  if (!box.querySelector('button')) box.innerHTML = '<strong>Something went wrong</strong><button class="copy-err-btn" id="btn-copy-error">Copy error</button>'
}

async function copyError () {
  const btn = document.getElementById('btn-copy-error')
  try {
    await navigator.clipboard.writeText(lastErrorText)
  } catch {
    const ta = document.createElement('textarea') // older webviews only allow copy via a selection
    ta.value = lastErrorText
    document.body.appendChild(ta); ta.select(); document.execCommand('copy'); ta.remove()
  }
  if (btn) { btn.textContent = 'Copied ✓'; setTimeout(() => { btn.textContent = 'Copy error' }, 1500) }
  announceStatus('Copied')
}

function applyData (data) {
  data = data || {}
  state.dataLoaded = true
  // pick up profile changes made during the poll (e.g. a GOG avatar fetched for an older sign-in)
  window.api.getAccounts().then(a => { if (JSON.stringify(a) !== JSON.stringify(state.accounts)) setAccounts(a) })
  state.freeGames    = data.freeGames    ?? []
  state.freeWeekends = data.freeWeekends ?? []
  state.deals        = data.deals        ?? []
  state.dealsSource  = data.dealsSource  ?? ''
  state.dealsError   = data.dealsError   ?? null
  state.sourceErrors = data.sourceErrors ?? []
  state.sales        = { steam: [], gog: [], epic: [], ...(data.sales ?? {}) }
  state.liveSale     = data.liveSale ?? null
  syncSaleTheme()
  state.wishlist     = data.wishlist ?? []
  state.wishlistErrors  = data.wishlistErrors ?? {}
  state.wishlistPrivate = !!data.wishlistPrivate
  if (state.selectedWishlistDeal && !state.wishlist.some(w => w.appId === state.selectedWishlistDeal.appId)) {
    state.selectedWishlistDeal = null
  }
  state.lastChecked  = data.lastChecked  ?? Date.now()
  updateRefreshAge()
  renderErrors()
  updateBadges()
  // Re-render the visible page — but never the settings page, so a background
  // poll can't yank toggles/inputs out from under the user
  if (state.page !== 'settings') navigate(state.page)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function escHtml (str) {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

// ─── Boot ─────────────────────────────────────────────────────────────────────

async function init () {
  // Titlebar
  document.getElementById('btn-min').addEventListener('click',   () => window.api.minimize())
  document.getElementById('btn-close').addEventListener('click', () => window.api.close())

  // Sidebar navigation
  document.querySelectorAll('.nav-item').forEach(el =>
    el.addEventListener('click', () => navigate(el.dataset.page))
  )

  // Check-now buttons
  document.getElementById('sidebar-errors')?.addEventListener('click', e => { if (e.target.closest('#btn-copy-error')) copyError() })
  document.getElementById('btn-check-now')?.addEventListener('click',      () => loadData(true))
  document.getElementById('btn-check-settings')?.addEventListener('click', () => loadData(true))
  document.getElementById('btn-show-onboarding')?.addEventListener('click', () => startOnboarding())
  document.getElementById('btn-reset-settings')?.addEventListener('click', async e => {
    const btn = e.currentTarget
    if (!resetSettingsArmed) {
      resetSettingsArmed = true
      btn.textContent = 'Really reset?'
      btn.setAttribute('aria-label', 'Confirm reset settings to defaults')
      clearTimeout(resetSettingsTimer)
      resetSettingsTimer = setTimeout(() => {
        resetSettingsArmed = false
        resetSettingsTimer = null
        btn.textContent = 'Reset settings to defaults'
        btn.setAttribute('aria-label', 'Reset settings to defaults')
      }, 4000)
      return
    }
    clearTimeout(resetSettingsTimer); resetSettingsArmed = false; btn.disabled = true
    resetSettings()
    try {
      await window.api.saveSettings(state.settings)
      applyTheme(); navigate('free', false)
    } finally { btn.disabled = false }
  })
  bindListControls('deals', 'deals-search', 'deals-sort', renderDeals, { minDiscountId: 'deals-discount-filters' })
  bindListControls('storesales', 'sales-search', 'sales-sort', renderStoreSales, { minDiscountId: 'sales-discount-filters' })
  bindListControls('wishlist', 'wishlist-search', 'wishlist-sort', renderWishlist, { saleOnlyId: 'wishlist-sale-only' })

  // ── Links (only hard-coded official URLs carry data-href) ───────────────
  document.addEventListener('click', e => {
    const a = e.target.closest('a[data-href]')
    if (a) { e.preventDefault(); window.api.openUrl(a.dataset.href) }
  })
  document.addEventListener('keydown', handleShortcutKeydown)
  document.addEventListener('click', e => {
    const target = e.target.closest?.('[data-open-sale-index]')
    const sale = target && SALES[Number(target.dataset.openSaleIndex)]
    if (sale) openSale(sale)
  })
  document.addEventListener('keydown', e => {
    const target = e.target
    if (e.key !== 'Enter' && e.key !== ' ') return
    const row = target.closest?.('[data-keyboard-action="activate"]')
    if (!row || (target !== row && target.closest?.('button, a, input, select, textarea'))) return
    e.preventDefault()
    row.click()
  })
  document.getElementById('btn-github')?.addEventListener('click', () => openBrowser(REPO))
  document.getElementById('btn-feedback')?.addEventListener('click', () => openBrowser(REPO + '/issues'))
  document.querySelectorAll('.rz').forEach(el => el.addEventListener('pointerdown', e => {
    if (e.button !== 0) return
    e.preventDefault()
    window.__TAURI__.window.getCurrentWindow().startResizeDragging(el.dataset.dir)
  }))
  window.__TAURI__.app.getVersion().then(v => {
    document.getElementById('app-version').textContent = 'DealDrop v' + v
    document.getElementById('about-version').textContent = 'DealDrop v' + v
  }).catch(() => {})

  // ── Load initial state ────────────────────────────────────────────────────
  Object.assign(state.settings, await window.api.getSettings())
  applyTheme()
  state.launchers = await window.api.launchers().catch(() => state.launchers)
  setAccounts(await window.api.getAccounts())

  // Listen for background push updates from main process
  window.api.onDataUpdate(data => applyData(data))

  // Render calendar (no API call needed) then kick off initial data fetch
  renderSalesCalendar()
  navigate(state.page)
  startRefreshAgeTimer()
  loadData()

  if (!state.settings.onboarded) startOnboarding()
}

document.addEventListener('DOMContentLoaded', init)
