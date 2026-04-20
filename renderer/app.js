'use strict'

const SALES_2026 = [
  { name: 'Lunar New Year Sale', start: '2026-02-06', end: '2026-02-13', icon: '🧧' },
  { name: 'Spring Sale',         start: '2026-03-19', end: '2026-03-26', icon: '🌸' },
  { name: 'Summer Sale',         start: '2026-06-26', end: '2026-07-10', icon: '☀️', est: true },
  { name: 'Halloween Sale',      start: '2026-10-29', end: '2026-11-02', icon: '🎃', est: true },
  { name: 'Autumn Sale',         start: '2026-11-25', end: '2026-12-02', icon: '🍂', est: true },
  { name: 'Winter Sale',         start: '2026-12-19', end: '2027-01-02', icon: '❄️', est: true },
]

const state = {
  page: 'free',
  freeGames:    [],
  freeWeekends: [],
  deals:        [],
  wishlist:     [],
  settings:     { notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true, notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60 },
  steamUser:    null,
  lastChecked:  null,
}

// ─── Utils ────────────────────────────────────────────────────────────────────

const fmt$ = n => n === 0 ? 'Free' : '$' + n.toFixed(2)

function timeLeft (dateStr) {
  if (!dateStr) return ''
  const d = Math.floor((new Date(dateStr) - Date.now()) / 86400000)
  if (d <= 0) return 'ended'
  return d === 1 ? '1d left' : `${d}d left`
}

function fmtChecked (ts) {
  if (!ts) return ''
  const s = Math.floor((Date.now() - ts) / 1000)
  if (s < 10) return 'updated just now'
  if (s < 60) return `updated ${s}s ago`
  if (s < 3600) return `updated ${Math.floor(s/60)}m ago`
  return `updated ${Math.floor(s/3600)}h ago`
}

function srcIcon (source) {
  return { steam: '🎮', epic: '🕹️', gog: '⚔️' }[source] ?? '🎮'
}

function el (html) {
  const t = document.createElement('template')
  t.innerHTML = html.trim()
  return t.content.firstChild
}

// ─── Open helpers ─────────────────────────────────────────────────────────────

function openSteamClient (appId) { window.api.openSteam(appId) }
function openBrowser     (url)   { window.api.openUrl(url) }

// ─── Shared card actions ──────────────────────────────────────────────────────

function makeCardActions (appId, url) {
  const wrap = document.createElement('div')
  wrap.className = 'card-actions'
  // Open in Steam client button
  const steamBtn = el(`<button class="card-btn steam-btn" title="Open in Steam client">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"><rect width="24" height="24" rx="3" fill="currentColor"/></svg>
    Steam app
  </button>`)
  steamBtn.addEventListener('click', e => { e.stopPropagation(); if (appId) openSteamClient(appId) })

  // Open in browser button
  const webBtn = el(`<button class="card-btn" title="Open in browser">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
    Browser
  </button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })

  wrap.append(steamBtn, webBtn)
  return wrap
}

function makeIconBtns (appId, url) {
  const wrap = document.createElement('div')
  wrap.className = 'wishlist-actions'

  const steamBtn = el(`<button class="icon-btn steam-icon-btn" title="Open in Steam client">
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"><rect width="24" height="24" rx="3" fill="#1b2838"/><path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="currentColor"/><circle cx="16" cy="12" r="2" fill="currentColor"/></svg>
  </button>`)
  steamBtn.addEventListener('click', e => { e.stopPropagation(); openSteamClient(appId) })

  const webBtn = el(`<button class="icon-btn" title="Open in browser">
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
  </button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })

  wrap.append(steamBtn, webBtn)
  return wrap
}

// ─── Page: Wishlist ───────────────────────────────────────────────────────────

function renderWishlist () {
  const prompt      = document.getElementById('wishlist-login-prompt')
  const onSaleWrap  = document.getElementById('wishlist-on-sale')
  const allWrap     = document.getElementById('wishlist-all-section')
  const listSale    = document.getElementById('list-wishlist-sale')
  const listAll     = document.getElementById('list-wishlist-all')
  const emptyEl     = document.getElementById('empty-wishlist')
  const tsEl        = document.getElementById('lc-wishlist')
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.steamUser) {
    prompt.style.display     = 'flex'
    onSaleWrap.style.display = 'none'
    allWrap.style.display    = 'none'
    emptyEl.style.display    = 'none'
    return
  }
  prompt.style.display = 'none'

  if (state.wishlist.length === 0) {
    onSaleWrap.style.display = 'none'
    allWrap.style.display    = 'none'
    emptyEl.style.display    = 'flex'
    return
  }
  emptyEl.style.display = 'none'

  listSale.innerHTML = ''
  listAll.innerHTML  = ''

  const onSale = state.wishlist.filter(w => (w.priceInfo?.discount ?? 0) >= 20)
  const badge  = document.getElementById('badge-wishlist')
  if (badge) { badge.textContent = onSale.length; badge.classList.toggle('visible', onSale.length > 0) }

  if (onSale.length > 0) {
    onSaleWrap.style.display = 'block'
    onSale.forEach(w => listSale.appendChild(makeWishlistDealRow(w)))
  } else {
    onSaleWrap.style.display = 'none'
  }

  allWrap.style.display = 'block'
  state.wishlist.forEach(w => listAll.appendChild(makeWishlistRow(w)))
}

function makeWishlistDealRow (w) {
  const p   = w.priceInfo
  const url = `https://store.steampowered.com/app/${w.appId}`
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${w.name}</div>
      <div class="deal-store">steam</div>
    </div>
    <span class="deal-orig">${fmt$(p.initial)}</span>
    <span class="deal-cut">-${p.discount}%</span>
    <span class="deal-price">${p.formatted}</span>`
  const openBtn = el(`<button class="deal-open" title="Open in Steam client">
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none"><rect width="24" height="24" rx="3" fill="#1b2838"/><path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="currentColor"/><circle cx="16" cy="12" r="2" fill="currentColor"/></svg>
  </button>`)
  openBtn.addEventListener('click', e => { e.stopPropagation(); openSteamClient(w.appId) })
  row.appendChild(openBtn)
  row.addEventListener('click', () => openBrowser(url))
  return row
}

function makeWishlistRow (w) {
  const p      = w.priceInfo
  const url    = `https://store.steampowered.com/app/${w.appId}`
  const onSale = (p?.discount ?? 0) >= 20
  const row    = document.createElement('div')
  row.className = 'wishlist-row' + (onSale ? ' on-sale' : '')

  const thumb = document.createElement('img')
  thumb.className = 'wishlist-thumb'
  thumb.src = w.capsule ? `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${w.appId}/capsule_sm_120.jpg` : ''
  thumb.onerror = function() { this.style.display = 'none' }

  const info = document.createElement('div')
  info.style.cssText = 'min-width:0;flex:1'
  const sub = p
    ? (onSale ? `<span style="color:var(--amber)">-${p.discount}% · ${p.formatted}</span>` : `<span>${p.formatted}</span>`)
    : ''
  info.innerHTML = `<div class="wishlist-name" title="${w.name}">${w.name}</div><div class="wishlist-sub">${sub}</div>`

  row.append(thumb, info, makeIconBtns(w.appId, url))
  row.addEventListener('click', () => openBrowser(url))
  return row
}

// ─── Page: Free Now ───────────────────────────────────────────────────────────

function renderFreeGames () {
  const gridFree     = document.getElementById('grid-free')
  const gridWeekends = document.getElementById('grid-weekends')
  const labelWknd    = document.getElementById('label-weekends')
  const emptyEl      = document.getElementById('empty-free')
  const tsEl         = document.getElementById('lc-free')
  gridFree.innerHTML = ''; gridWeekends.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.freeGames.length && !state.freeWeekends.length) { emptyEl.style.display = 'flex'; return }
  emptyEl.style.display = 'none'

  state.freeGames.forEach(g => gridFree.appendChild(makeFreeCard(g, 'tag-free', 'FREE')))
  if (state.freeWeekends.length) {
    labelWknd.style.display = 'block'
    state.freeWeekends.forEach(g => gridWeekends.appendChild(makeFreeCard(g, 'tag-weekend', 'WEEKEND')))
  } else { labelWknd.style.display = 'none' }

  const total = state.freeGames.length + state.freeWeekends.length
  const b     = document.getElementById('badge-free')
  if (b) { b.textContent = total; b.classList.toggle('visible', total > 0) }
}

function makeFreeCard (g, tagClass, tagText) {
  const card = document.createElement('div')
  card.className = 'game-card'
  card.innerHTML = `
    <div class="game-thumb">
      ${g.image ? `<img src="${g.image}" alt="${g.title}" onerror="this.parentElement.innerHTML='<span class=fallback>${srcIcon(g.source)}</span>'" />` : `<span class="fallback">${srcIcon(g.source)}</span>`}
      <span class="src-badge ${g.source}">${g.source.toUpperCase()}</span>
    </div>
    <div class="game-info">
      <div class="game-title" title="${g.title}">${g.title}</div>
      <div class="price-row">
        <span class="${tagClass}">${tagText}</span>
        ${g.endDate ? `<span class="time-left">${timeLeft(g.endDate)}</span>` : ''}
      </div>
    </div>`
  card.appendChild(makeCardActions(g.appId ?? null, g.url))
  return card
}

// ─── Page: Deals ─────────────────────────────────────────────────────────────

function renderDeals () {
  const list    = document.getElementById('list-deals')
  const notice  = document.getElementById('itad-notice')
  const emptyEl = document.getElementById('empty-deals')
  const tsEl    = document.getElementById('lc-deals')
  list.innerHTML = ''
  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.settings.itadKeySet) { notice.style.display = 'flex'; emptyEl.style.display = 'none'; return }
  notice.style.display = 'none'
  if (!state.deals.length) { emptyEl.style.display = 'flex'; return }
  emptyEl.style.display = 'none'
  state.deals.forEach(d => list.appendChild(makeDealRow(d)))
}

function makeDealRow (d) {
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${d.title}</div>
      <div class="deal-store">${d.shop}</div>
    </div>
    <span class="deal-orig">${fmt$(d.regular)}</span>
    <span class="deal-cut">-${d.cut}%</span>
    <span class="deal-price">${fmt$(d.price)}</span>`
  const btn = el(`<button class="deal-open" title="Open in browser">
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
  </button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openBrowser(d.url) })
  row.appendChild(btn)
  row.addEventListener('click', () => openBrowser(d.url))
  return row
}

// ─── Page: Sales Calendar ─────────────────────────────────────────────────────

function renderSalesCalendar () {
  const banner   = document.getElementById('next-sale-banner')
  const timeline = document.getElementById('sale-timeline')
  const now      = new Date()
  timeline.innerHTML = ''
  const next = SALES_2026.find(s => new Date(s.start) > now)
  if (next) {
    const d = Math.ceil((new Date(next.start) - now) / 86400000)
    banner.innerHTML = `<div class="nsb-icon">${next.icon}</div><div class="nsb-info"><div class="nsb-title">${next.name}</div><div class="nsb-sub">${next.est ? 'Estimated — based on past years' : 'Confirmed date'}</div></div><div class="nsb-countdown"><div class="nsb-val">${d}</div><div class="nsb-lbl">days away</div></div>`
  }
  SALES_2026.forEach(s => {
    const start  = new Date(s.start)
    const end    = new Date(s.end)
    const isPast = end < now
    const isLive = start <= now && end >= now
    const isNext = s === next
    const fmt    = d => d.toLocaleDateString('en-US', { month:'short', day:'numeric' })
    const item   = document.createElement('div')
    item.className = 'timeline-item' + (isPast ? ' past' : '') + (isNext ? ' next' : '')
    item.innerHTML = `
      <div class="tl-dot"></div>
      <span class="tl-name">${s.icon} ${s.name}${isLive ? ' <span style="color:var(--green);font-size:11px">(live now!)</span>' : ''}${s.est ? ' <span style="font-size:10px;color:var(--text2)">(est.)</span>' : ''}</span>
      <span class="tl-date">${fmt(start)} – ${fmt(end)}</span>`
    timeline.appendChild(item)
  })
}

// ─── Page: Settings ───────────────────────────────────────────────────────────

function renderSettings () {
  const s   = state.settings
  const con = document.getElementById('notif-settings')
  con.innerHTML = ''
  const rows = [
    { key: 'notifyMajorSale',    label: 'Steam major sale started',  sub: 'Summer, Winter, Autumn & Spring sales' },
    { key: 'notifyFreeGame',     label: 'Free game available',        sub: 'Epic, Steam & GOG giveaways' },
    { key: 'notifyFreeWeekend',  label: 'Free weekend started',       sub: 'Temporary Steam free-to-play promotions' },
    { key: 'notifyWishlistSale', label: 'Wishlist game on sale',      sub: 'Notify when synced wishlist games drop in price' },
    { key: 'notifyBigDeal',      label: 'Deal over 75% off',          sub: 'Requires IsThereAnyDeal API key' },
  ]
  rows.forEach(r => {
    const row = document.createElement('div')
    row.className = 'setting-row'
    row.innerHTML = `<div><div class="setting-label">${r.label}</div><div class="setting-sub">${r.sub}</div></div><button class="toggle ${s[r.key] ? 'on' : ''}" data-key="${r.key}"></button>`
    row.querySelector('.toggle').addEventListener('click', e => {
      e.currentTarget.classList.toggle('on')
      s[r.key] = e.currentTarget.classList.contains('on')
      window.api.saveSettings(s)
    })
    con.appendChild(row)
  })
  const ig = document.getElementById('interval-group')
  ig.innerHTML = ''
  ;[15, 60, 240, 720].forEach(mins => {
    const b = document.createElement('button')
    b.className = 'interval-btn' + (s.checkInterval === mins ? ' active' : '')
    b.textContent = mins < 60 ? mins + ' min' : (mins/60) + ' hr'
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
    case 'wishlist':  renderWishlist();       break
    case 'free':      renderFreeGames();      break
    case 'deals':     renderDeals();          break
    case 'sales':     renderSalesCalendar();  break
    case 'settings':  renderSettings();       break
  }
}

// ─── Steam account UI ────────────────────────────────────────────────────────

function updateSteamUI (user) {
  state.steamUser = user
  const loggedOut = document.getElementById('steam-logged-out')
  const loggedIn  = document.getElementById('steam-logged-in')
  if (user) {
    loggedOut.style.display = 'none'
    loggedIn.style.display  = 'block'
    const username = document.getElementById('steam-username')
    const avatar   = document.getElementById('steam-avatar')
    if (username) username.textContent = user.name
    if (avatar) {
      if (user.avatar) { avatar.src = user.avatar; avatar.style.display = 'block' }
      else {
        avatar.style.display = 'none'
        const ph = loggedIn.querySelector('.steam-avatar-placeholder') ?? document.createElement('div')
        ph.className = 'steam-avatar-placeholder'; ph.textContent = '👤'
        avatar.parentNode?.insertBefore(ph, avatar)
      }
    }
  } else {
    loggedOut.style.display = 'block'
    loggedIn.style.display  = 'none'
  }
  // Re-render wishlist if it's the current page
  if (state.page === 'wishlist') renderWishlist()
}

// ─── Load & data ──────────────────────────────────────────────────────────────

async function loadData () {
  const dot   = document.getElementById('status-dot')
  const label = document.getElementById('status-label')
  dot.classList.add('active')
  if (label) label.textContent = 'checking…'
  try {
    const data = await window.api.fetchData()
    applyData(data)
  } catch (e) { console.error('fetchData', e) }
  if (label) label.textContent = 'monitoring'
}

function applyData (data) {
  state.freeGames    = data.freeGames    ?? []
  state.freeWeekends = data.freeWeekends ?? []
  state.deals        = data.deals        ?? []
  state.wishlist     = data.wishlist     ?? []
  state.lastChecked  = data.lastChecked  ?? Date.now()
  navigate(state.page)
}

// ─── Init ─────────────────────────────────────────────────────────────────────

async function init () {
  document.getElementById('btn-min').addEventListener('click',   () => window.api.minimize())
  document.getElementById('btn-close').addEventListener('click', () => window.api.close())

  document.querySelectorAll('.nav-item').forEach(el => el.addEventListener('click', () => navigate(el.dataset.page)))

  // Check-now
  document.getElementById('btn-check-now').addEventListener('click', () => loadData())
  document.getElementById('btn-check-settings')?.addEventListener('click', () => loadData())

  // Steam login / logout
  const doLogin = async () => {
    const btn = document.getElementById('btn-steam-login')
    if (btn) { btn.textContent = 'Opening Steam…'; btn.disabled = true }
    const res = await window.api.steamLogin()
    if (res.ok) updateSteamUI(res.profile)
    else alert('Steam sign-in failed: ' + res.error)
    if (btn) { btn.textContent = 'Sign in via Steam'; btn.disabled = false }
  }
  document.getElementById('btn-steam-login')?.addEventListener('click', doLogin)
  document.getElementById('btn-steam-login-main')?.addEventListener('click', doLogin)
  document.getElementById('btn-steam-logout')?.addEventListener('click', async () => {
    await window.api.steamLogout()
    updateSteamUI(null)
    state.wishlist = []
  })

  // ITAD key
  const itadInput = document.getElementById('itad-key-input')
  const itadKey   = await window.api.getItadKey()
  state.settings.itadKeySet = !!itadKey
  if (itadInput) itadInput.value = itadKey ? '••••••••' : ''
  document.getElementById('save-itad-key')?.addEventListener('click', async () => {
    const v = itadInput.value.trim()
    if (!v || v === '••••••••') return
    await window.api.saveItadKey(v)
    state.settings.itadKeySet = true
    itadInput.value = '••••••••'
    loadData()
  })

  // Steam Web API key
  const sApiInput = document.getElementById('steam-api-key-input')
  const sKey      = await window.api.getSteamApiKey()
  if (sApiInput) sApiInput.value = sKey ? '••••••••' : ''
  document.getElementById('save-steam-api-key')?.addEventListener('click', async () => {
    const v = sApiInput.value.trim()
    if (!v || v === '••••••••') return
    await window.api.saveSteamApiKey(v)
    sApiInput.value = '••••••••'
  })

  // Hint links
  document.querySelectorAll('.hint a[data-href]').forEach(a => {
    a.addEventListener('click', e => { e.preventDefault(); window.api.openUrl(a.dataset.href) })
  })

  // Load settings & steam user
  const saved = await window.api.getSettings()
  Object.assign(state.settings, saved)
  state.settings.itadKeySet = !!itadKey

  const user = await window.api.getSteamUser()
  updateSteamUI(user)

  // Background updates from main process
  window.api.onDataUpdate(data => applyData(data))

  // Initial render
  renderSalesCalendar()
  navigate('free')
  loadData()

  // Refresh timestamps periodically
  setInterval(() => {
    const tsEl = document.getElementById('lc-' + state.page)
    if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)
  }, 30000)
}

document.addEventListener('DOMContentLoaded', init)
