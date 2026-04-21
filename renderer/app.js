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
  page:         'free',
  freeGames:    [],
  freeWeekends: [],
  deals:        [],
  wishlist:     [],
  settings:     {
    notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true,
    notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60,
  },
  steamUser:    null,
  itadKeySet:   false,
  lastChecked:  null,
}

// ─── Utilities ────────────────────────────────────────────────────────────────

const fmt$ = n => n === 0 ? 'Free' : '$' + n.toFixed(2)

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

  const steamBtn = tmpl(`<button class="card-btn steam-btn" title="Open in Steam client">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none">
      <rect width="24" height="24" rx="3" fill="#1b2838"/>
      <path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="#c7d5e0"/>
      <circle cx="16" cy="12" r="2" fill="#c7d5e0"/>
    </svg>
    Steam app
  </button>`)
  steamBtn.addEventListener('click', e => {
    e.stopPropagation()
    source === 'steam' && appId ? openSteam(appId) : openBrowser(url)
  })

  const webBtn = tmpl(`<button class="card-btn" title="Open in browser">
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="10"/>
      <line x1="2" y1="12" x2="22" y2="12"/>
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
    </svg>
    Browser
  </button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })

  wrap.append(steamBtn, webBtn)
  return wrap
}

function makeIconBtns (appId, url) {
  const wrap     = document.createElement('div')
  wrap.className = 'wishlist-actions'

  const steamBtn = tmpl(`<button class="icon-btn steam-icon-btn" title="Open in Steam client">
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
      <rect width="24" height="24" rx="3" fill="#1b2838"/>
      <path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="currentColor"/>
      <circle cx="16" cy="12" r="2" fill="currentColor"/>
    </svg>
  </button>`)
  steamBtn.addEventListener('click', e => { e.stopPropagation(); openSteam(appId) })

  const webBtn = tmpl(`<button class="icon-btn" title="Open in browser">
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="10"/>
      <line x1="2" y1="12" x2="22" y2="12"/>
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
    </svg>
  </button>`)
  webBtn.addEventListener('click', e => { e.stopPropagation(); openBrowser(url) })

  wrap.append(steamBtn, webBtn)
  return wrap
}

// ─── Wishlist page ────────────────────────────────────────────────────────────

function renderWishlist () {
  const loginPrompt  = document.getElementById('wishlist-login-prompt')
  const onSaleWrap   = document.getElementById('wishlist-on-sale')
  const allWrap      = document.getElementById('wishlist-all-section')
  const listSale     = document.getElementById('list-wishlist-sale')
  const listAll      = document.getElementById('list-wishlist-all')
  const emptyEl      = document.getElementById('empty-wishlist')
  const tsEl         = document.getElementById('lc-wishlist')

  if (tsEl) tsEl.textContent = fmtChecked(state.lastChecked)

  if (!state.steamUser) {
    loginPrompt.style.display  = 'flex'
    onSaleWrap.style.display   = 'none'
    allWrap.style.display      = 'none'
    emptyEl.style.display      = 'none'
    return
  }
  loginPrompt.style.display = 'none'

  if (!state.wishlist.length) {
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

  onSaleWrap.style.display = onSale.length > 0 ? 'block' : 'none'
  onSale.forEach(w => listSale.appendChild(makeWishlistSaleRow(w)))

  allWrap.style.display = 'block'
  state.wishlist.forEach(w => listAll.appendChild(makeWishlistRow(w)))
}

function makeWishlistSaleRow (w) {
  const p   = w.priceInfo
  const url = `https://store.steampowered.com/app/${w.appId}`
  const row = document.createElement('div')
  row.className = 'deal-row'
  row.innerHTML = `
    <div>
      <div class="deal-name">${escHtml(w.name)}</div>
      <div class="deal-store">steam</div>
    </div>
    <span class="deal-orig">${fmt$(p.initial)}</span>
    <span class="deal-cut">-${p.discount}%</span>
    <span class="deal-price">${p.formatted}</span>`

  const btn = tmpl(`<button class="deal-open" title="Open in Steam client">
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none">
      <rect width="24" height="24" rx="3" fill="#1b2838"/>
      <path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="currentColor"/>
      <circle cx="16" cy="12" r="2" fill="currentColor"/>
    </svg>
  </button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openSteam(w.appId) })
  row.appendChild(btn)
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
  thumb.src   = `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${w.appId}/capsule_sm_120.jpg`
  thumb.alt   = ''
  thumb.addEventListener('error', () => { thumb.style.display = 'none' })

  const info  = document.createElement('div')
  info.style.cssText = 'min-width:0;flex:1'
  const priceStr = p
    ? (onSale
        ? `<span style="color:var(--amber)">-${p.discount}% · ${p.formatted}</span>`
        : `<span style="color:var(--text2)">${p.formatted}</span>`)
    : ''
  info.innerHTML = `<div class="wishlist-name" title="${escHtml(w.name)}">${escHtml(w.name)}</div><div class="wishlist-sub">${priceStr}</div>`

  row.append(thumb, info, makeIconBtns(w.appId, url))
  row.addEventListener('click', () => openBrowser(url))
  return row
}

// ─── Free games page ──────────────────────────────────────────────────────────

function renderFreeGames () {
  const gridFree  = document.getElementById('grid-free')
  const gridWknd  = document.getElementById('grid-weekends')
  const lblWknd   = document.getElementById('label-weekends')
  const emptyEl   = document.getElementById('empty-free')
  const tsEl      = document.getElementById('lc-free')

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

  const total = state.freeGames.length + state.freeWeekends.length
  const badge = document.getElementById('badge-free')
  if (badge) { badge.textContent = total; badge.classList.toggle('visible', total > 0) }
}

function makeFreeCard (g, tagClass, tagText) {
  const card = document.createElement('div')
  card.className = 'game-card'
  const srcClass = ['steam','epic','gog'].includes(g.source) ? g.source : 'other'
  card.innerHTML = `
    <div class="game-thumb">
      ${g.image
        ? `<img src="${g.image}" alt="" onerror="this.style.display='none'">`
        : `<span class="fallback">${srcIcon(g.source)}</span>`}
      <span class="src-badge ${srcClass}">${(g.source ?? 'other').toUpperCase()}</span>
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

  if (!state.itadKeySet) {
    notice.style.display  = 'flex'
    emptyEl.style.display = 'none'
    return
  }
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
      <div class="deal-name">${escHtml(d.title)}</div>
      <div class="deal-store">${escHtml(d.shop)}</div>
    </div>
    <span class="deal-orig">${fmt$(d.regular)}</span>
    <span class="deal-cut">-${d.cut}%</span>
    <span class="deal-price">${fmt$(d.price)}</span>`
  const btn = tmpl(`<button class="deal-open" title="Open store page">
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
      <polyline points="15 3 21 3 21 9"/>
      <line x1="10" y1="14" x2="21" y2="3"/>
    </svg>
  </button>`)
  btn.addEventListener('click', e => { e.stopPropagation(); openBrowser(d.url) })
  row.appendChild(btn)
  row.addEventListener('click', () => openBrowser(d.url))
  return row
}

// ─── Sales calendar ───────────────────────────────────────────────────────────

function renderSalesCalendar () {
  const banner   = document.getElementById('next-sale-banner')
  const timeline = document.getElementById('sale-timeline')
  const now      = new Date()
  timeline.innerHTML = ''

  const next = SALES_2026.find(s => new Date(s.start) > now)
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

  const fmtDate = d => d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
  SALES_2026.forEach(s => {
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
    { key: 'notifyWishlistSale', label: 'Wishlist game on sale',       sub: 'Notify when synced wishlist games drop ≥20%' },
    { key: 'notifyBigDeal',      label: 'Deal over 75% off',           sub: 'Requires IsThereAnyDeal API key' },
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
    case 'deals':     renderDeals();         break
    case 'sales':     renderSalesCalendar(); break
    case 'settings':  renderSettings();      break
  }
}

// ─── Steam account UI ────────────────────────────────────────────────────────

function updateSteamUI (user) {
  state.steamUser = user
  const loggedOut = document.getElementById('steam-logged-out')
  const loggedIn  = document.getElementById('steam-logged-in')

  if (!user) {
    loggedOut.style.display = 'block'
    loggedIn.style.display  = 'none'
    if (state.page === 'wishlist') renderWishlist()
    return
  }

  loggedOut.style.display = 'none'
  loggedIn.style.display  = 'block'

  const usernameEl = document.getElementById('steam-username')
  const avatarEl   = document.getElementById('steam-avatar')
  const avatarPh   = document.getElementById('steam-avatar-placeholder')

  if (usernameEl) usernameEl.textContent = user.name ?? 'Steam user'

  if (user.avatar) {
    if (avatarEl)  { avatarEl.src = user.avatar; avatarEl.style.display = 'block' }
    if (avatarPh)    avatarPh.style.display = 'none'
  } else {
    if (avatarEl)  avatarEl.style.display = 'none'
    if (avatarPh)  avatarPh.style.display = 'flex'
  }

  if (state.page === 'wishlist') renderWishlist()
}

// ─── Data loading ─────────────────────────────────────────────────────────────

async function loadData () {
  const dot   = document.getElementById('status-dot')
  const label = document.getElementById('status-label')
  if (dot)   dot.classList.add('active')
  if (label) label.textContent = 'checking…'
  try {
    const data = await window.api.fetchData()
    applyData(data)
  } catch (e) {
    console.error('[fetchData]', e)
  } finally {
    if (label) label.textContent = 'monitoring'
  }
}

function applyData (data) {
  state.freeGames    = data.freeGames    ?? []
  state.freeWeekends = data.freeWeekends ?? []
  state.deals        = data.deals        ?? []
  state.wishlist     = data.wishlist     ?? []
  state.lastChecked  = data.lastChecked  ?? Date.now()
  navigate(state.page)
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
  document.getElementById('btn-check-now')?.addEventListener('click',      () => loadData())
  document.getElementById('btn-check-settings')?.addEventListener('click', () => loadData())

  // ── Steam login / logout ──────────────────────────────────────────────────
  const doLogin = async () => {
    const btns = [
      document.getElementById('btn-steam-login'),
      document.getElementById('btn-steam-login-main'),
    ].filter(Boolean)
    btns.forEach(b => { b.textContent = 'Opening Steam…'; b.disabled = true })
    try {
      const res = await window.api.steamLogin()
      if (res.ok) {
        updateSteamUI(res.profile)
        await loadData()   // refresh wishlist immediately after login
      } else {
        alert('Steam sign-in failed: ' + (res.error ?? 'unknown error'))
      }
    } finally {
      btns.forEach(b => {
        b.disabled = false
        if (b.id === 'btn-steam-login') b.innerHTML = svgSteamIcon() + ' Sign in via Steam'
        else b.textContent = 'Sign in via Steam'
      })
    }
  }
  document.getElementById('btn-steam-login')?.addEventListener('click',      doLogin)
  document.getElementById('btn-steam-login-main')?.addEventListener('click', doLogin)

  document.getElementById('btn-steam-logout')?.addEventListener('click', async () => {
    await window.api.steamLogout()
    state.wishlist = []
    updateSteamUI(null)
  })

  // ── ITAD API key ──────────────────────────────────────────────────────────
  const itadInput = document.getElementById('itad-key-input')
  const itadKey   = await window.api.getItadKey()
  state.itadKeySet = !!itadKey
  if (itadInput) itadInput.value = itadKey ? '••••••••' : ''

  document.getElementById('save-itad-key')?.addEventListener('click', async () => {
    const v = itadInput?.value?.trim()
    if (!v || v === '••••••••') return
    await window.api.saveItadKey(v)
    state.itadKeySet = true
    if (itadInput) itadInput.value = '••••••••'
    loadData()
  })

  // ── Steam Web API key ─────────────────────────────────────────────────────
  const sApiInput = document.getElementById('steam-api-key-input')
  const sKey      = await window.api.getSteamApiKey()
  if (sApiInput) sApiInput.value = sKey ? '••••••••' : ''

  document.getElementById('save-steam-api-key')?.addEventListener('click', async () => {
    const v = sApiInput?.value?.trim()
    if (!v || v === '••••••••') return
    await window.api.saveSteamApiKey(v)
    if (sApiInput) sApiInput.value = '••••••••'
    // Refresh profile with new key so name/avatar update immediately
    const refreshed = await window.api.refreshProfile()
    if (refreshed) updateSteamUI(refreshed)
  })

  // ── Hint links ────────────────────────────────────────────────────────────
  document.querySelectorAll('.hint a[data-href]').forEach(a =>
    a.addEventListener('click', e => { e.preventDefault(); window.api.openUrl(a.dataset.href) })
  )

  // ── Load initial state ────────────────────────────────────────────────────
  const saved = await window.api.getSettings()
  Object.assign(state.settings, saved)
  state.itadKeySet = !!itadKey

  const user = await window.api.getSteamUser()
  updateSteamUI(user)

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

function svgSteamIcon () {
  return `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="flex-shrink:0">
    <rect width="24" height="24" rx="3" fill="#1b2838"/>
    <path d="M12 3C7.03 3 3 7.03 3 12c0 4.09 2.6 7.58 6.26 8.9l2.27-4.54A3 3 0 1 1 15 12h.05l2.6-5.22A9 9 0 0 0 12 3z" fill="#c7d5e0"/>
    <circle cx="16" cy="12" r="2" fill="#c7d5e0"/>
  </svg>`
}

document.addEventListener('DOMContentLoaded', init)
