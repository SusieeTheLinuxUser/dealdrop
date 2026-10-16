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
  salesTab:     'steam',
  wishlistTab:  'all',
  wishlistErrors: {},
  wishlist:     [],
  settings:     {
    notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true,
    notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60,
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

function tmpl (html) {
  const t = document.createElement('template')
  t.innerHTML = html.trim()
  return t.content.firstChild
}

// ─── Open helpers ─────────────────────────────────────────────────────────────

const openSteam   = appId => window.api.openSteam(appId)
const openBrowser = url   => window.api.openUrl(url)

// ─── Shared action buttons ────────────────────────────────────────────────────

function makeCardActions (appId, url, source) {
  const wrap     = document.createElement('div')
  wrap.className = 'card-actions'

  const steamBtn = source !== 'steam' || !appId ? null : tmpl(`<button class="card-btn steam-btn" title="Open in Steam client">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none">
      <rect width="24" height="24" rx="3" fill="#1b2838"/>
      <path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="#c7d5e0"/>
      <circle cx="16" cy="12" r="2" fill="#c7d5e0"/>
    </svg>
    Steam app
  </button>`)
  steamBtn?.addEventListener('click', e => { e.stopPropagation(); openSteam(appId) })

  const webBtn = tmpl(`<button class="card-btn" title="Open in browser">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="10"/>
      <line x1="2" y1="12" x2="22" y2="12"/>
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
    </svg>
    Browser
  </button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })

  if (steamBtn) wrap.append(steamBtn)
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

function makeWishlistSaleRow (w) {
  const p   = w.priceInfo
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${escHtml(w.name)}</div>
      <div class="deal-store">${storeTag(w.store)}</div>
    </div>
    <span class="deal-orig">${fmt$(p.initial)}</span>
    <span class="deal-cut">-${p.discount}%</span>
    <span class="deal-price">${escHtml(p.formatted ?? fmt$(p.final))}</span>`
  const btn = tmpl(`<button class="deal-open" title="Open store page">↗</button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openWishlistItem(w) })
  row.appendChild(btn)
  row.addEventListener('click', () => openBrowser(w.url))
  return row
}

const openWishlistItem = w => w.store === 'steam' ? openSteam(w.appId) : openBrowser(w.url)

function makeWishlistRow (w) {
  const p      = w.priceInfo
  const onSale = (p?.discount ?? 0) >= 20
  const row    = document.createElement('div')
  row.className = 'wishlist-row' + (onSale ? ' on-sale' : '')

  const thumb = document.createElement('img')
  thumb.className = 'wishlist-thumb'
  thumb.src   = w.image ?? ''
  thumb.alt   = ''
  thumb.addEventListener('error', () => { thumb.style.visibility = 'hidden' })

  const info  = document.createElement('div')
  info.style.cssText = 'min-width:0;flex:1'
  const priceStr = p?.formatted
    ? (onSale
        ? `<span style="color:var(--amber)">-${p.discount}% · ${escHtml(p.formatted)}</span>`
        : `<span style="color:var(--text2)">${escHtml(p.formatted)}</span>`)
    : ''
  info.innerHTML = `<div class="wishlist-name" title="${escHtml(w.name)}">${storeTag(w.store)}${escHtml(w.name)}</div><div class="wishlist-sub">${priceStr}</div>`

  row.append(thumb, info, makeIconBtns(w))
  row.addEventListener('click', () => w.store === 'steam' ? selectWishlistGame(w) : openBrowser(w.url))
  return row
}

function makeIconBtns (w) {
  const wrap     = document.createElement('div')
  wrap.className = 'wishlist-actions'
  if (w.store === 'steam') {
    const steamBtn = tmpl(`<button class="icon-btn steam-icon-btn" title="Open in Steam client">S</button>`)
    steamBtn.addEventListener('click', e => { e.stopPropagation(); openSteam(w.appId) })
    wrap.append(steamBtn)
  }
  const webBtn = tmpl(`<button class="icon-btn" title="Open in browser">↗</button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(w.url) })
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
        ? `<img src="${escHtml(g.image)}" alt="" onerror="this.style.display='none'">`
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
  card.appendChild(makeCardActions(g.appId ?? null, g.url, g.source))
  return card
}

// ─── Deals page ───────────────────────────────────────────────────────────────

function renderDeals () {
  const list    = document.getElementById('list-deals')
  const notice  = document.getElementById('itad-notice')
  const emptyEl = document.getElementById('empty-deals')
  const tsEl    = document.getElementById('lc-deals')

  list.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  // Say why, instead of an unexplained empty list (e.g. ITAD rejecting the key)
  const note = state.dealsError ?? (state.dealsSource ? `Source: ${state.dealsSource}` : '')
  notice.classList.toggle('err', !!state.dealsError)
  notice.style.display = note ? 'flex' : 'none'
  notice.textContent = state.dealsError ? `${state.dealsError}${state.dealsSource ? ` Showing ${state.dealsSource} instead.` : ''}` : note

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
      ${d.image ? `<img src="${escHtml(d.image)}" alt="" onerror="this.style.display='none'">` : `<span class="fallback">${srcIcon(src)}</span>`}
      <span class="src-badge ${src}">${escHtml(d.shop)}</span>
    </div>
    <div class="game-info">
      <div class="game-title" title="${escHtml(d.title)}">${escHtml(d.title)}</div>
      <div class="price-row">
        <span class="tag-sale">-${d.cut}%</span>
        <span class="price-was">${fmt$(d.regular)}</span>
        <span class="price-now">${fmt$(d.price)}</span>
      </div>
    </div>`
  card.appendChild(makeCardActions(d.appId ?? null, d.url, src === 'steam' ? 'steam' : src))
  return card
}

// ─── Sales calendar ───────────────────────────────────────────────────────────

function renderSalesCalendar () {
  const banner   = document.getElementById('next-sale-banner')
  const timeline = document.getElementById('sale-timeline')
  const now      = new Date()
  timeline.innerHTML = ''

  const next = SALES.find(s => new Date(s.start) > now)
  if (next) {
    const days = Math.ceil((new Date(next.start) - now) / 86400000)
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
    item.className = 'timeline-item' + (isPast ? ' past' : '') + (isNext ? ' next' : '')
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

function renderSettings () {
  const s   = state.settings
  const con = document.getElementById('notif-settings')
  con.innerHTML = ''

  const rows = [
    { key: 'notifyMajorSale',    label: 'Steam major sale started',   sub: 'Summer, Winter, Autumn & Spring sales' },
    { key: 'notifyFreeGame',     label: 'Free game available',         sub: 'Epic, Steam, GOG & GamerPower giveaways' },
    { key: 'notifyFreeWeekend',  label: 'Free weekend started',        sub: 'Temporary Steam free-to-play promotions' },
    { key: 'notifyWishlistSale', label: 'Wishlist game on sale',       sub: 'Steam, GOG & Epic wishlists, drops of ≥20%' },
    { key: 'notifyBigDeal',      label: 'Deal over 75% off',           sub: 'Hot deals list (CheapShark, or ITAD with a key)' },
  ]

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
    })
    con.appendChild(row)
  })

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

  // ── Hint links ────────────────────────────────────────────────────────────
  document.querySelectorAll('.hint a[data-href]').forEach(a =>
    a.addEventListener('click', e => { e.preventDefault(); window.api.openUrl(a.dataset.href) })
  )

  // ── Load initial state ────────────────────────────────────────────────────
  Object.assign(state.settings, await window.api.getSettings())
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