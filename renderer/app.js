'use strict'

const SALES = [
  { name: 'Lunar New Year Sale', start: '2026-02-06', end: '2026-02-13', icon: '🧧' },
  { name: 'Spring Sale',         start: '2026-03-19', end: '2026-03-26', icon: '🌸' },
  { name: 'Summer Sale',         start: '2026-06-26', end: '2026-07-10', icon: '☀️', est: true },
  { name: 'Halloween Sale',      start: '2026-10-29', end: '2026-11-02', icon: '🎃', est: true },
  { name: 'Autumn Sale',         start: '2026-11-25', end: '2026-12-02', icon: '🍂', est: true },
  { name: 'Winter Sale',         start: '2026-12-19', end: '2027-01-02', icon: '❄️', est: true },
  { name: 'Lunar New Year Sale', start: '2027-02-04', end: '2027-02-11', icon: '🧧', est: true },
  { name: 'Spring Sale',         start: '2027-03-18', end: '2027-03-25', icon: '🌸', est: true },
  { name: 'Summer Sale',         start: '2027-06-24', end: '2027-07-08', icon: '☀️', est: true },
]

const state = {
  page:         'free',
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
  settings:     {
    notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true,
    notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60,
    theme: 'midnight', baseTheme: 'midnight', lastSaleTheme: '', rounded: false, effects: true, launcherGog: 'auto', launcherEpic: 'auto',
  },
  accounts:     { steam: null, gog: null, epic: null },
  lastChecked:  null,
  selectedWishlistDeal: null,
  wishlistPrivate: false,
}

// ─── Utilities ────────────────────────────────────────────────────────────────

const fmt$ = n => {
  const num = Number(n) || 0
  return num === 0 ? 'Free' : '$' + num.toFixed(2)
}

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
const launcherOf  = store => state.launchers[store]?.chosen ?? null

// ─── Shared action buttons ────────────────────────────────────────────────────

// Buttons: "<Store> app" only when that launcher is installed (the backend falls back
// to the browser anyway), plus Browser.
function makeCardActions (store, url, id) {
  const wrap     = document.createElement('div')
  wrap.className = 'card-actions'
  const ch = launcherOf(store)
  if (ch) {
    const appBtn = tmpl(`<button class="card-btn steam-btn" title="Open in ${ch.label}">${ch.label}</button>`)
    appBtn.addEventListener('click', e => { e.stopPropagation(); window.api.openInApp(store, url, id) })
    wrap.append(appBtn)
  }
  const webBtn = tmpl(`<button class="card-btn" title="Open in browser">🌐 Browser</button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })
  wrap.append(webBtn)
  return wrap
}

// ─── Wishlist page ────────────────────────────────────────────────────────────

function renderWishlist () {
  const $ = id => document.getElementById(id)
  const loginPrompt = $('wishlist-login-prompt'), onSaleWrap = $('wishlist-on-sale')
  const allWrap = $('wishlist-all-section'), emptyEl = $('empty-wishlist')
  const panel = $('wishlist-deal-panel'), tabs = $('wishlist-tabs'), errs = $('wishlist-errors')

  updateBadges()
  if ($('lc-wishlist')) $('lc-wishlist').textContent = fmtChecked(state.lastChecked)
  const hide = (...els) => els.forEach(e => { if (e) e.style.display = 'none' })

  if (!hasAccount()) {
    loginPrompt.style.display = 'flex'
    hide(onSaleWrap, allWrap, emptyEl, panel, tabs, errs)
    return
  }
  hide(loginPrompt)

  // Per-store failures (e.g. Epic rejecting the token) shown instead of silently empty
  const failed = Object.entries(state.wishlistErrors).filter(([s]) => state.accounts[s])
  errs.classList.add('err')
  errs.style.display = failed.length ? 'flex' : 'none'
  errs.textContent = failed.map(([s, e]) => `${STORES[s]} wishlist: ${e}`).join('  ·  ')

  // Store filter tabs (only for signed-in stores)
  const stores = Object.keys(STORES).filter(s => state.accounts[s])
  if (!stores.includes(state.wishlistTab)) state.wishlistTab = 'all'
  tabs.style.display = stores.length > 1 ? 'flex' : 'none'
  tabs.innerHTML = ''
  ;['all', ...stores].forEach(s => {
    const n = s === 'all' ? state.wishlist.length : state.wishlist.filter(w => w.store === s).length
    const b = document.createElement('button')
    b.className = 'tab' + (state.wishlistTab === s ? ' active' : '')
    b.textContent = `${s === 'all' ? 'All' : STORES[s]} (${n})`
    b.addEventListener('click', () => { state.wishlistTab = s; renderWishlist() })
    tabs.appendChild(b)
  })

  const items = state.wishlistTab === 'all' ? state.wishlist : state.wishlist.filter(w => w.store === state.wishlistTab)
  if (!items.length) {
    hide(onSaleWrap, allWrap, panel)
    const steamOnly = state.wishlistTab === 'steam' || (state.wishlistTab === 'all' && state.accounts.steam && !state.accounts.gog && !state.accounts.epic)
    $('wishlist-empty-msg').textContent = steamOnly && state.wishlistPrivate
      ? 'Your Steam wishlist is set to Private. Make it Public in Steam → Privacy Settings.'
      : failed.length ? 'Wishlist could not be loaded (see above).' : 'Wishlist is empty.'
    emptyEl.style.display = 'flex'
    return
  }
  emptyEl.style.display = 'none'

  const listSale = $('list-wishlist-sale'), listAll = $('list-wishlist-all')
  listSale.innerHTML = ''; listAll.innerHTML = ''
  const onSale = items.filter(w => (w.priceInfo?.discount ?? 0) >= 20)
  onSaleWrap.style.display = onSale.length ? 'block' : 'none'
  onSale.forEach(w => listSale.appendChild(makeWishlistSaleRow(w)))
  allWrap.style.display = 'block'
  items.forEach(w => listAll.appendChild(makeWishlistRow(w)))
  panel.style.display = state.selectedWishlistDeal ? 'block' : 'none'
  renderWishlistDealPanel()
}

const storeTag = s => `<span class="store-tag ${s}">${STORES[s] ?? s}</span>`

// Where "open" should go: the cheapest known offer if another store beats this one
const bestUrl = w => w.cheapest?.url ?? w.url

function makeWishlistSaleRow (w) {
  const p   = w.priceInfo
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${escHtml(w.name)}</div>
      <div class="deal-store">${storeTag(w.store)}${w.cheapest ? `<span class="cheapest-tag">↘ ${escHtml(w.cheapest.store)} ${fmt$(w.cheapest.price)}</span>` : ''}</div>
    </div>
    <span class="deal-orig">${fmt$(p.initial)}</span>
    <span class="deal-cut">-${p.discount}%</span>
    <span class="deal-price">${escHtml(p.formatted ?? fmt$(p.final))}</span>`
  const btn = tmpl(`<button class="deal-open" title="${w.cheapest ? 'Open cheapest offer' : 'Open store page'}">↗</button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openBrowser(bestUrl(w)) })
  row.appendChild(btn)
  row.addEventListener('click', () => openBrowser(bestUrl(w)))
  return row
}

function makeWishlistRow (w) {
  const p      = w.priceInfo
  const onSale = (p?.discount ?? 0) >= 20
  const row    = document.createElement('div')
  row.className = 'wishlist-row' + (onSale ? ' on-sale' : '')

  const thumb = document.createElement('img')
  thumb.className = 'wishlist-thumb'
  thumb.src   = w.image ?? ''
  thumb.alt   = ''
  thumb.dataset.fb = ''
  thumb.style.visibility = w.image ? '' : 'hidden'

  const info  = document.createElement('div')
  info.style.cssText = 'min-width:0;flex:1'
  const priceStr = p?.formatted
    ? (onSale
        ? `<span style="color:var(--amber)">-${p.discount}% · ${escHtml(p.formatted)}</span>`
        : `<span style="color:var(--text2)">${escHtml(p.formatted)}</span>`)
    : ''
  const cheap = w.cheapest ? ` <span class="cheapest-tag">↘ cheaper at ${escHtml(w.cheapest.store)}: ${fmt$(w.cheapest.price)}</span>` : ''
  info.innerHTML = `<div class="wishlist-name" title="${escHtml(w.name)}">${storeTag(w.store)}${escHtml(w.name)}</div><div class="wishlist-sub">${priceStr}${cheap}</div>`

  row.append(thumb, info, makeIconBtns(w))
  row.addEventListener('click', () => w.store === 'steam' ? selectWishlistGame(w) : openBrowser(bestUrl(w)))
  return row
}

function makeIconBtns (w) {
  const wrap     = document.createElement('div')
  wrap.className = 'wishlist-actions'
  const ch = launcherOf(w.store)
  if (ch) {
    const appBtn = tmpl(`<button class="icon-btn steam-icon-btn" title="Open in ${ch.label}">${ch.label[0]}</button>`)
    appBtn.addEventListener('click', e => { e.stopPropagation(); window.api.openInApp(w.store, w.url, w.id) })
    wrap.append(appBtn)
  }
  const webBtn = tmpl(`<button class="icon-btn" title="${w.cheapest ? `Open cheapest offer (${escHtml(w.cheapest.store)})` : 'Open in browser'}">${w.cheapest ? '↘' : '↗'}</button>`)
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

  panel.querySelector('#wishlist-deal-title').textContent = d.gameName ?? `App ${d.appId}`
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
  panel.querySelector('#wishlist-open-steam').onclick = () => openSteam(d.appId)
}

// ─── Free games page ──────────────────────────────────────────────────────────

function renderFreeGames () {
  const gridFree  = document.getElementById('grid-free')
  const gridWknd  = document.getElementById('grid-weekends')
  const lblWknd   = document.getElementById('label-weekends')
  const emptyEl   = document.getElementById('empty-free')
  const tsEl      = document.getElementById('lc-free')

  updateBadges()
  gridFree.innerHTML = ''
  gridWknd.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.freeGames.length && !state.freeWeekends.length) {
    emptyEl.style.display = 'flex'
    return
  }
  emptyEl.style.display = 'none'
  state.freeGames.forEach(g => gridFree.appendChild(makeFreeCard(g, 'tag-free', 'FREE')))

  if (state.freeWeekends.length) {
    lblWknd.style.display = 'block'
    state.freeWeekends.forEach(g => gridWknd.appendChild(makeFreeCard(g, 'tag-weekend', 'WEEKEND')))
  } else {
    lblWknd.style.display = 'none'
  }
}

// Sidebar badges — computed from state so they stay correct even when the
// owning page isn't the one being rendered right now
function updateBadges () {
  const onSale = onSaleItems().length
  const wb = document.getElementById('badge-wishlist')
  if (wb) { wb.textContent = onSale; wb.classList.toggle('visible', onSale > 0) }

  const total = state.freeGames.length + state.freeWeekends.length
  const fb = document.getElementById('badge-free')
  if (fb) { fb.textContent = total; fb.classList.toggle('visible', total > 0) }
}

function makeFreeCard (g, tagClass, tagText) {
  const card = document.createElement('div')
  card.className = 'game-card'
  const srcClass = ['steam','epic','gog'].includes(g.source) ? g.source : 'other'
  card.innerHTML = `
    <div class="game-thumb">
      ${g.image
        ? `<img src="${escHtml(g.image)}" alt="" data-fb="${srcIcon(g.source)}">`
        : `<span class="fallback">${srcIcon(g.source)}</span>`}
      <span class="src-badge ${srcClass}">${escHtml(g.source ?? 'other').toUpperCase()}</span>
    </div>
    <div class="game-info">
      <div class="game-title" title="${escHtml(g.title)}">${escHtml(g.title)}</div>
      <div class="price-row">
        <span class="${tagClass}">${tagText}</span>
        ${g.endDate ? `<span class="time-left">${timeLeft(g.endDate)}</span>` : ''}
      </div>
    </div>`
  card.appendChild(makeCardActions(g.source, g.url, g.appId))
  return card
}

// ─── Deals page ───────────────────────────────────────────────────────────────

const ITAD_HELP = `<div>To get a key: <ol>
  <li>Open <a data-href="https://isthereanydeal.com/apps/my/">isthereanydeal.com/apps/my</a> and register an app.</li>
  <li>Pick <strong>“This app will NOT store OAuth secret securely”</strong>.</li>
  <li>Copy the key under <strong>API Keys</strong> (not Client ID / Secret) and paste it in Settings.</li></ol></div>`

function renderDeals () {
  const list    = document.getElementById('list-deals')
  const notice  = document.getElementById('itad-notice')
  const emptyEl = document.getElementById('empty-deals')
  const tsEl    = document.getElementById('lc-deals')

  list.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  // Say why, instead of an unexplained empty list (e.g. ITAD rejecting the key)
  notice.classList.toggle('err', !!state.dealsError)
  notice.style.display = state.dealsError || state.dealsSource ? 'flex' : 'none'
  notice.innerHTML = state.dealsError
    ? `<div>${escHtml(state.dealsError)}${state.dealsSource ? ` Showing ${escHtml(state.dealsSource)} instead.` : ''}</div>${ITAD_HELP}`
    : `Source: ${escHtml(state.dealsSource)}`

  emptyEl.style.display = state.deals.length ? 'none' : 'flex'
  state.deals.forEach(d => list.appendChild(makeDealRow(d)))
}

function makeDealRow (d) {
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${escHtml(d.title)}</div>
      <div class="deal-store">${escHtml(d.shop)}</div>
    </div>
    <span class="deal-orig">${fmt$(d.regular)}</span>
    <span class="deal-cut">-${d.cut}%</span>
    <span class="deal-price">${fmt$(d.price)}</span>`
  const btn = tmpl(`<button class="deal-open" title="Open store page">↗</button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openBrowser(d.url) })
  row.appendChild(btn)
  row.addEventListener('click', () => openBrowser(d.url))
  return row
}

// ─── Store sales page ─────────────────────────────────────────────────────────

function renderStoreSales () {
  const tabs = document.getElementById('sales-tabs')
  const grid = document.getElementById('grid-storesales')
  const emptyEl = document.getElementById('empty-storesales')
  document.getElementById('lc-storesales').textContent = fmtChecked(state.lastChecked)

  tabs.innerHTML = ''
  Object.keys(STORES).forEach(s => {
    const b = document.createElement('button')
    b.className = 'tab' + (state.salesTab === s ? ' active' : '')
    b.textContent = `${STORES[s]} (${state.sales[s]?.length ?? 0})`
    b.addEventListener('click', () => { state.salesTab = s; renderStoreSales() })
    tabs.appendChild(b)
  })

  grid.innerHTML = ''
  const list = state.sales[state.salesTab] ?? []
  emptyEl.style.display = list.length ? 'none' : 'flex'
  list.forEach(d => grid.appendChild(makeSaleCard(d)))
}

function makeSaleCard (d) {
  const src = d.shop.toLowerCase()
  const card = document.createElement('div')
  card.className = 'game-card'
  card.innerHTML = `
    <div class="game-thumb">
      ${d.image ? `<img src="${escHtml(d.image)}" alt="" data-fb="${srcIcon(src)}">` : `<span class="fallback">${srcIcon(src)}</span>`}
      <span class="src-badge ${escHtml(src)}">${escHtml(d.shop)}</span>
    </div>
    <div class="game-info">
      <div class="game-title" title="${escHtml(d.title)}">${escHtml(d.title)}</div>
      <div class="price-row">
        <span class="tag-sale">-${d.cut}%</span>
        <span class="price-was">${fmt$(d.regular)}</span>
        <span class="price-now">${fmt$(d.price)}</span>
      </div>
    </div>`
  card.appendChild(makeCardActions(src, d.url, d.appId ?? d.id))
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
    banner.onclick = () => openSale(next)
    banner.innerHTML = `
      <div class="nsb-icon">${next.icon}</div>
      <div class="nsb-info">
        <div class="nsb-title">${next.name}</div>
        <div class="nsb-sub">${next.est ? 'Estimated — based on past years' : 'Confirmed date'}</div>
      </div>
      <div class="nsb-countdown">
        <div class="nsb-val">${days}</div>
        <div class="nsb-lbl">days away</div>
      </div>`
  } else {
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
    item.title = 'Open on Steam'
    item.addEventListener('click', () => openSale(s))
    item.innerHTML = `
      <div class="tl-dot"></div>
      <span class="tl-name">
        ${s.icon} ${s.name}
        ${isLive ? '<span style="color:var(--green);font-size:11px;margin-left:4px">(live now!)</span>' : ''}
        ${s.est  ? '<span style="font-size:10px;color:var(--text2);margin-left:4px">(est.)</span>'      : ''}
      </span>
      <span class="tl-date">${fmtDate(start)} – ${fmtDate(end)}</span>`
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

// A sale starting switches to its theme once. If the user picks something else, lastSaleTheme
// stays set, so a restart doesn't switch back. When the sale ends its theme is dropped.
function syncSaleTheme () {
  const s = state.settings
  const key = state.liveSale?.key ?? ''
  let dirty = false
  if (key && SALE_THEMES[key]) {
    if (s.lastSaleTheme !== key) { s.lastSaleTheme = key; s.theme = key; dirty = true }
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
    row.innerHTML = `
      <div>
        <div class="setting-label">${r.label}</div>
        <div class="setting-sub">${r.sub}</div>
      </div>
      <button class="toggle ${s[r.key] ? 'on' : ''}" data-key="${r.key}"></button>`
    row.querySelector('.toggle').addEventListener('click', e => {
      const btn = e.currentTarget
      btn.classList.toggle('on')
      s[r.key] = btn.classList.contains('on')
      window.api.saveSettings(s)
      after?.()
    })
    con.appendChild(row)
  })
}

// "Open GOG/Epic games in": Auto prefers the store's own client, then Heroic, then Lutris
function renderLauncherPrefs () {
  const con = document.getElementById('launcher-prefs')
  con.innerHTML = ''
  ;[['gog', 'launcherGog', 'GOG'], ['epic', 'launcherEpic', 'Epic Games']].forEach(([store, key, name]) => {
    const row = document.createElement('div')
    row.className = 'setting-row'
    row.innerHTML = `<div><div class="setting-label">Open ${name} games in</div>
      <div class="setting-sub">${state.launchers[store]?.options.length ? 'Detected: ' + state.launchers[store].options.map(o => o.label).join(', ') : 'No launcher detected — opens in the browser'}</div></div><div class="theme-grid" style="margin:0"></div>`
    const grid = row.querySelector('.theme-grid')
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
  toggleRows(document.getElementById('look-settings'), [
    { key: 'rounded', label: 'Rounded window',  sub: 'Round the corners of the app window' },
    { key: 'effects', label: 'Seasonal effects', sub: 'Falling leaves, snow & more during Steam sale themes' },
  ], applyTheme)

  renderLauncherPrefs()
  toggleRows(document.getElementById('notif-settings'), [
    { key: 'notifyMajorSale',    label: 'Steam major sale started',   sub: 'Summer, Winter, Autumn & Spring sales' },
    { key: 'notifyFreeGame',     label: 'Free game available',         sub: 'Epic, Steam, GOG & GamerPower giveaways' },
    { key: 'notifyFreeWeekend',  label: 'Free weekend started',        sub: 'Temporary Steam free-to-play promotions' },
    { key: 'notifyWishlistSale', label: 'Wishlist game on sale',       sub: 'Steam, GOG & Epic wishlists, drops of ≥20%' },
    { key: 'notifyBigDeal',      label: 'Deal over 75% off',           sub: 'Hot deals list (CheapShark, or ITAD with a key)' },
  ])

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
}

// ─── Navigation ───────────────────────────────────────────────────────────────

function navigate (page) {
  state.page = page
  document.querySelectorAll('.nav-item').forEach(el => el.classList.toggle('active', el.dataset.page === page))
  document.querySelectorAll('.page').forEach(el => el.classList.toggle('active', el.id === 'page-' + page))
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

const ACCT_ICON = { steam: 'ST', gog: 'GOG', epic: 'EGS' }

function renderAccounts () {
  const con = document.getElementById('accounts')
  con.innerHTML = ''
  Object.keys(STORES).forEach(s => {
    const a = state.accounts[s]
    const name = s === 'steam' ? a?.name : a
    const row = document.createElement('div')
    row.className = 'acct'
    row.innerHTML = `<span class="acct-ico ${s}">${ACCT_ICON[s]}</span>` + (a
      ? `<div class="acct-main"><span class="acct-name">${escHtml(name || STORES[s] + ' user')}</span><button class="acct-out">Sign out</button></div>`
      : `<span class="acct-label">${STORES[s]}</span><button class="acct-btn">Sign in</button>`)
    row.querySelector('.acct-out')?.addEventListener('click', async () => {
      setAccounts(await window.api.logout(s))
      state.wishlist = state.wishlist.filter(w => w.store !== s)
      navigate(state.page)
    })
    const inBtn = row.querySelector('.acct-btn')
    inBtn?.addEventListener('click', async () => {
      inBtn.disabled = true; inBtn.textContent = 'Opening…'
      try {
        setAccounts(await window.api.login(s))
        await loadData()
      } catch (e) {
        if (String(e) !== 'Cancelled') alert(`${STORES[s]} sign-in failed: ${e}`)
      } finally {
        renderAccounts()
      }
    })
    con.appendChild(row)
  })
}

function setAccounts (acc) {
  state.accounts = { steam: acc?.steam ?? null, gog: acc?.gog ?? null, epic: acc?.epic ?? null }
  renderAccounts()
  if (state.page === 'wishlist') renderWishlist()
}

// ─── Data loading ─────────────────────────────────────────────────────────────

async function loadData (force = false) {
  const dot   = document.getElementById('status-dot')
  const label = document.getElementById('status-label')
  if (dot)   dot.classList.add('active')
  if (label) label.textContent = 'checking…'
  try {
    if (force) state.launchers = await window.api.launchers().catch(() => state.launchers)
    applyData(await (force ? window.api.checkNow() : window.api.fetchData()))
  } catch (e) {
    console.error('[fetchData]', e)
  } finally {
    if (label) label.textContent = 'monitoring'
  }
}

function applyData (data) {
  data = data || {}
  state.freeGames    = data.freeGames    ?? []
  state.freeWeekends = data.freeWeekends ?? []
  state.deals        = data.deals        ?? []
  state.dealsSource  = data.dealsSource  ?? ''
  state.dealsError   = data.dealsError   ?? null
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
  document.getElementById('btn-check-now')?.addEventListener('click',      () => loadData(true))
  document.getElementById('btn-check-settings')?.addEventListener('click', () => loadData(true))

  // ── API keys ──────────────────────────────────────────────────────────────
  const keyInput = (id, name, btnId, after) => {
    const input = document.getElementById(id)
    window.api.hasKey(name).then(has => { if (input) input.value = has ? '••••••••' : '' })
    document.getElementById(btnId)?.addEventListener('click', async () => {
      const v = input?.value?.trim()
      if (!v || v === '••••••••') return
      const btn = document.getElementById(btnId)
      btn.disabled = true
      try {
        await window.api.saveKey(name, v)
        input.value = '••••••••'
        await after()
      } catch (e) {
        alert(String(e))
      } finally {
        btn.disabled = false
      }
    })
  }
  keyInput('itad-key-input', 'itad', 'save-itad-key', async () => { const d = await window.api.checkNow(); applyData(d) })
  keyInput('steam-api-key-input', 'steam', 'save-steam-api-key', async () => {
    const refreshed = await window.api.refreshProfile()
    if (refreshed) setAccounts(await window.api.getAccounts())
  })

  // ── Links (only hard-coded official URLs carry data-href) ───────────────
  document.addEventListener('click', e => {
    const a = e.target.closest('a[data-href]')
    if (a) { e.preventDefault(); window.api.openUrl(a.dataset.href) }
  })
  document.getElementById('btn-github')?.addEventListener('click', () => openBrowser(REPO))
  document.getElementById('btn-feedback')?.addEventListener('click', () => openBrowser(REPO + '/issues'))
  window.__TAURI__.app.getVersion().then(v => { document.getElementById('app-version').textContent = 'DealDrop v' + v }).catch(() => {})

  // ── Load initial state ────────────────────────────────────────────────────
  Object.assign(state.settings, await window.api.getSettings())
  applyTheme()
  state.launchers = await window.api.launchers().catch(() => state.launchers)
  setAccounts(await window.api.getAccounts())

  // Listen for background push updates from main process
  window.api.onDataUpdate(data => applyData(data))

  // Render calendar (no API call needed) then kick off initial data fetch
  renderSalesCalendar()
  navigate('free')
  loadData()

  // Refresh "updated X ago" text every 30s
  setInterval(() => {
    const tsEl = document.getElementById('lc-' + state.page)
    if (tsEl && state.lastChecked) tsEl.textContent = fmtChecked(state.lastChecked)
  }, 30_000)
}

document.addEventListener('DOMContentLoaded', init)