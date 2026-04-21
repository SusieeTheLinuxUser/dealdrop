const {
  app, BrowserWindow, Tray, Menu, nativeImage,
  ipcMain, Notification, shell
} = require('electron')
const path  = require('path')
const Store = require('electron-store')

const store = new Store()

const DEFAULT_SETTINGS = {
  notifyMajorSale:    true,
  notifyFreeGame:     true,
  notifyFreeWeekend:  true,
  notifyWishlistSale: true,
  notifyBigDeal:      false,
  checkInterval:      60,
}

let mainWindow  = null
let tray        = null
let pollTimer   = null
let lastSaleKey = store.get('lastSaleKey', null)
let seenFreeIds = new Set(store.get('seenFreeIds', []))
let seenSaleIds = new Set(store.get('seenSaleIds', []))

// cached data so renderer gets something instantly on open
let cachedData  = null

// ─── Fetch helpers ────────────────────────────────────────────────────────────

async function get (url, extraHeaders = {}) {
  const res = await fetch(url, {
    headers: {
      'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36',
      'Accept':     'application/json, text/plain, */*',
      'Accept-Language': 'en-US,en;q=0.9',
      ...extraHeaders,
    },
  })
  if (!res.ok) throw new Error(`HTTP ${res.status} — ${url}`)
  return res.json()
}

async function getText (url) {
  const res = await fetch(url, {
    headers: { 'User-Agent': 'DealDrop/0.2.0' },
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.text()
}

// ─── Steam OpenID auth ────────────────────────────────────────────────────────

function steamLogin () {
  return new Promise((resolve, reject) => {
    const RETURN_URL = 'https://dealdrop.local/auth/return'
    const params = new URLSearchParams({
      'openid.mode':       'checkid_setup',
      'openid.ns':         'http://specs.openid.net/auth/2.0',
      'openid.identity':   'http://specs.openid.net/auth/2.0/identifier_select',
      'openid.claimed_id': 'http://specs.openid.net/auth/2.0/identifier_select',
      'openid.return_to':  RETURN_URL,
      'openid.realm':      'https://dealdrop.local',
    })

    const win = new BrowserWindow({
      width: 820, height: 640,
      parent: mainWindow, modal: true,
      title: 'Sign in through Steam',
      backgroundColor: '#1b2838',
      webPreferences: { nodeIntegration: false, contextIsolation: true },
    })

    win.setMenu(null)
    win.loadURL(`https://steamcommunity.com/openid/login?${params}`)

    const STEAM_ID_RE = /openid\.identity=https?%3A%2F%2Fsteamcommunity\.com%2Fopenid%2Fid%2F(\d+)/

    const check = (url) => {
      if (!url.startsWith(RETURN_URL)) return
      const match = url.match(STEAM_ID_RE)
      if (match) {
        win.destroy()
        resolve(match[1])
      } else {
        win.destroy()
        reject(new Error('Steam did not return a valid identity URL'))
      }
    }

    win.webContents.on('will-redirect', (_, url) => check(url))
    win.webContents.on('did-navigate',  (_, url) => check(url))
    win.on('closed', () => reject(new Error('Cancelled')))
  })
}

// ─── Steam profile ────────────────────────────────────────────────────────────
// Strategy: try Web API key first (best), then fall back to public XML profile.

async function fetchSteamProfile (steamId) {
  const apiKey = store.get('steamApiKey', '')

  // 1. Try Steam Web API (requires key)
  if (apiKey) {
    try {
      const data = await get(
        `https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key=${apiKey}&steamids=${steamId}`
      )
      const p = data?.response?.players?.[0]
      if (p) return { steamId, name: p.personaname, avatar: p.avatarmedium ?? p.avatar }
    } catch (e) {
      console.warn('[profile] Web API failed, falling back to XML:', e.message)
    }
  }

  // 2. Fall back to public XML profile — no API key needed
  try {
    const xml = await getText(`https://steamcommunity.com/profiles/${steamId}/?xml=1`)
    const name   = xml.match(/<steamID><!?\[?CDATA\[?([^\]<]+)\]?\]?>/i)?.[1]?.trim()
                ?? xml.match(/<steamID>([^<]+)<\/steamID>/i)?.[1]?.trim()
    const avatar = xml.match(/<avatarMedium><!?\[?CDATA\[?([^\]<]+)\]?\]?>/i)?.[1]?.trim()
                ?? xml.match(/<avatarMedium>([^<]+)<\/avatarMedium>/i)?.[1]?.trim()
    if (name) return { steamId, name, avatar: avatar ?? null }
  } catch (e) {
    console.warn('[profile] XML fallback failed:', e.message)
  }

  // 3. Last resort — show Steam ID tail, better than crashing
  return { steamId, name: `Steam user ${steamId.slice(-4)}`, avatar: null }
}

// ─── Wishlist ─────────────────────────────────────────────────────────────────
// Strategy: IStoreService (requires key, returns full data) → wishlistdata endpoint.

async function fetchWishlist (steamId) {
  const apiKey = store.get('steamApiKey', '')

  // 1. IStoreService — accurate, no pagination issues, requires Web API key
  if (apiKey) {
    try {
      const data = await get(
        `https://api.steampowered.com/IStoreService/GetWishlist/v1/?key=${apiKey}&steamid=${steamId}`
      )
      const items = data?.response?.items ?? []
      if (items.length > 0) {
        return items.map(i => ({
          appId:    String(i.appid),
          name:     i.name ?? `App ${i.appid}`,
          capsule:  true,
          priority: i.priority ?? 999,
        })).sort((a, b) => a.priority - b.priority)
      }
    } catch (e) {
      console.warn('[wishlist] IStoreService failed, trying fallback:', e.message)
    }
  }

  // 2. Public wishlistdata endpoint — paginated, works for public profiles
  const items = []
  let page = 0
  while (true) {
    try {
      const data = await get(
        `https://store.steampowered.com/wishlist/profiles/${steamId}/wishlistdata/?p=${page}`,
        { 'Referer': `https://store.steampowered.com/wishlist/profiles/${steamId}/` }
      )
      // Steam returns {} or an empty object when there are no more pages
      if (!data || typeof data !== 'object' || Object.keys(data).length === 0) break

      // Sometimes Steam returns an error object
      if (data.success === 2 || data.rwgrsn === -2) {
        console.warn('[wishlist] Wishlist is private or does not exist')
        break
      }

      for (const [appId, info] of Object.entries(data)) {
        if (isNaN(Number(appId))) continue  // skip non-appId keys
        items.push({
          appId:    appId,
          name:     info.name ?? `App ${appId}`,
          capsule:  !!info.capsule,
          priority: info.priority ?? 999,
        })
      }

      page++
      if (Object.keys(data).length < 100) break  // last page
    } catch (e) {
      console.error('[wishlist] page fetch failed:', e.message)
      break
    }
  }

  return items.sort((a, b) => a.priority - b.priority)
}

// ─── Price checks ─────────────────────────────────────────────────────────────

async function checkPrices (appIds) {
  if (!appIds.length) return {}
  const results = {}
  const BATCH   = 50  // Steam supports up to ~50 per request

  for (let i = 0; i < appIds.length; i += BATCH) {
    const chunk = appIds.slice(i, i + BATCH).join(',')
    try {
      const data = await get(
        `https://store.steampowered.com/api/appdetails?appids=${chunk}&filters=price_overview&cc=us`
      )
      for (const [id, info] of Object.entries(data)) {
        if (info?.success && info.data?.price_overview) {
          const p      = info.data.price_overview
          results[id]  = {
            discount:  p.discount_percent,
            final:     p.final   / 100,
            initial:   p.initial / 100,
            formatted: p.final_formatted,
          }
        }
      }
    } catch (e) {
      console.warn('[prices] batch failed:', e.message)
    }
    // Polite delay between batches
    if (i + BATCH < appIds.length) await sleep(300)
  }

  return results
}

// ─── Free games ───────────────────────────────────────────────────────────────

async function fetchEpicFreeGames () {
  try {
    const data  = await get(
      'https://store-site-backend-static.ak.epicgames.com/freeGamesPromotions?locale=en-US&country=US&allowCountries=US'
    )
    const elems = data?.data?.Catalog?.searchStore?.elements ?? []
    return elems
      .filter(g => {
        const offer = g.promotions?.promotionalOffers?.[0]?.promotionalOffers?.[0]
        return offer?.discountSetting?.discountPercentage === 0
      })
      .map(g => {
        const offer = g.promotions.promotionalOffers[0].promotionalOffers[0]
        const slug  = g.productSlug ?? g.catalogNs?.mappings?.[0]?.pageSlug ?? ''
        return {
          id:      'epic-' + g.id,
          title:   g.title,
          source:  'epic',
          url:     `https://store.epicgames.com/p/${slug}`,
          endDate: offer.endDate,
          image:   g.keyImages?.find(i => i.type === 'Thumbnail')?.url ?? null,
          type:    'free',
        }
      })
  } catch (e) {
    console.error('[epic]', e.message)
    return []
  }
}

async function fetchSteamFeaturedFree () {
  try {
    const data  = await get('https://store.steampowered.com/api/featured/?cc=us&l=en')
    const items = [
      ...(data.large_capsules ?? []),
      ...(data.featured_win   ?? []),
    ]
    return items
      .filter(i => i.final_price === 0 && (i.original_price ?? 0) > 0)
      .map(i => ({
        id:     'steam-' + i.id,
        appId:  String(i.id),
        title:  i.name,
        source: 'steam',
        url:    `https://store.steampowered.com/app/${i.id}`,
        image:  i.large_capsule_image ?? null,
        type:   'free',
      }))
  } catch (e) {
    console.error('[steam-featured]', e.message)
    return []
  }
}

async function fetchSteamFreeWeekends () {
  try {
    const data = await get('https://store.steampowered.com/api/featuredcategories/?cc=us&l=en')
    return (data?.coming_soon?.items ?? [])
      .filter(i => i.discount_percent === 100)
      .map(i => ({
        id:     'steam-fw-' + i.id,
        appId:  String(i.id),
        title:  i.name,
        source: 'steam',
        url:    `https://store.steampowered.com/app/${i.id}`,
        image:  i.large_capsule_image ?? null,
        type:   'weekend',
      }))
  } catch (e) {
    console.error('[steam-weekends]', e.message)
    return []
  }
}

async function fetchGamerPowerGiveaways () {
  try {
    const data = await get('https://www.gamerpower.com/api/giveaways?platform=pc')
    if (!Array.isArray(data)) return []
    return data.map(g => ({
      id:      'gp-' + g.id,
      title:   g.title,
      source:  g.platforms?.toLowerCase().includes('steam') ? 'steam'
             : g.platforms?.toLowerCase().includes('epic')  ? 'epic'
             : 'other',
      appId:   g.steam_appid ? String(g.steam_appid) : null,
      url:     g.open_giveaway_url ?? g.open_giveaway ?? '#',
      endDate: g.end_date !== 'N/A' ? g.end_date : null,
      image:   g.thumbnail ?? null,
      type:    'free',
    }))
  } catch (e) {
    console.error('[gamerpower]', e.message)
    return []
  }
}

// ─── Sale detection ───────────────────────────────────────────────────────────

async function detectSteamSale () {
  try {
    const data       = await get('https://store.steampowered.com/api/featuredcategories/?cc=us&l=en')
    const specials   = data?.specials?.items ?? []
    const heavyCount = specials.filter(i => (i.discount_percent ?? 0) >= 50).length
    const spotlight  = data?.spotlight?.items ?? []
    const banner     = spotlight.find(i => /sale|fest|event/i.test(i.name ?? ''))
    const isSaleOn   = heavyCount >= 8 || !!banner
    return { isSaleOn, saleName: banner?.name ?? (isSaleOn ? 'Steam Sale' : null) }
  } catch (e) {
    console.error('[sale-detect]', e.message)
    return { isSaleOn: false, saleName: null }
  }
}

// ─── ITAD ─────────────────────────────────────────────────────────────────────

async function fetchITADDeals (apiKey) {
  if (!apiKey) return []
  try {
    // ITAD v2 deals endpoint — sort by highest discount descending
    const url  = `https://api.isthereanydeal.com/deals/v2?key=${encodeURIComponent(apiKey)}&limit=25&sort=cut%3Adesc`
    const data = await get(url)

    // Handle both possible response shapes
    const list = data?.list ?? data?.data?.list ?? []

    return list.map(d => ({
      id:      d.id ?? d.slug,
      title:   d.title,
      cut:     d.deal?.cut       ?? 0,
      price:   d.deal?.price?.amount   ?? 0,
      regular: d.deal?.regular?.amount ?? 0,
      shop:    d.deal?.shop?.name ?? 'unknown',
      url:     d.deal?.url ?? d.urls?.buy ?? '#',
    })).filter(d => d.cut > 0)
  } catch (e) {
    console.error('[itad]', e.message)
    return []
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const sleep = ms => new Promise(r => setTimeout(r, ms))

function notify (title, body) {
  if (!Notification.isSupported()) return
  const n = new Notification({ title, body })
  n.on('click', () => { mainWindow?.show(); mainWindow?.focus() })
  n.show()
}

// ─── Poll ─────────────────────────────────────────────────────────────────────

async function poll () {
  console.log('[poll] checking…')
  const settings = store.get('settings',   DEFAULT_SETTINGS)
  const itadKey  = store.get('itadApiKey', '')
  const steamId  = store.get('steamId',    null)

  // 1. Steam major sale detection
  if (settings.notifyMajorSale) {
    const { isSaleOn, saleName } = await detectSteamSale()
    const key = isSaleOn ? (saleName ?? 'sale') : null
    if (isSaleOn && key !== lastSaleKey) {
      lastSaleKey = key
      store.set('lastSaleKey', key)
      notify('🎉 Steam sale started!', `${saleName} is live — go grab some deals.`)
    } else if (!isSaleOn) {
      lastSaleKey = null
      store.set('lastSaleKey', null)
    }
  }

  // 2. Free game detection — fetch once, use for both notifications and renderer
  const [ef, sf, sw, gp] = await Promise.all([
    fetchEpicFreeGames(),
    fetchSteamFeaturedFree(),
    fetchSteamFreeWeekends(),
    fetchGamerPowerGiveaways(),
  ])

  if (settings.notifyFreeGame || settings.notifyFreeWeekend) {
    const checkNew = (games, type, key) => {
      if (!settings[key]) return
      const fresh = games.filter(g => !seenFreeIds.has(g.id))
      if (!fresh.length) return
      fresh.forEach(g => seenFreeIds.add(g.id))
      store.set('seenFreeIds', [...seenFreeIds])
      if (fresh.length === 1)
        notify(
          type === 'weekend' ? `Free weekend: ${fresh[0].title}` : `Free: ${fresh[0].title}`,
          `${fresh[0].title} is free on ${(fresh[0].source ?? 'a store').toUpperCase()}!`
        )
      else
        notify(
          `${fresh.length} free ${type === 'weekend' ? 'weekends' : 'games'} available`,
          fresh.map(g => g.title).join(', ')
        )
    }
    checkNew([...ef, ...sf, ...gp], 'free',    'notifyFreeGame')
    checkNew(sw,                    'weekend',  'notifyFreeWeekend')
  }

  // 3. Wishlist sale notifications
  let wishlist = []
  let prices   = {}
  if (steamId) {
    try {
      wishlist = await fetchWishlist(steamId)
      if (wishlist.length) {
        prices = await checkPrices(wishlist.map(w => w.appId))

        if (settings.notifyWishlistSale) {
          const onSale = wishlist.filter(w => (prices[w.appId]?.discount ?? 0) >= 20)
          const fresh  = onSale.filter(w => !seenSaleIds.has(w.appId))
          if (fresh.length) {
            fresh.forEach(w => seenSaleIds.add(w.appId))
            store.set('seenSaleIds', [...seenSaleIds])
            if (fresh.length === 1) {
              const p = prices[fresh[0].appId]
              notify(`🏷️ ${fresh[0].name} is on sale!`, `${p.discount}% off — now ${p.formatted} on Steam`)
            } else {
              notify(
                `${fresh.length} wishlist games on sale`,
                fresh.slice(0, 4).map(w => w.name).join(', ') + (fresh.length > 4 ? '…' : '')
              )
            }
          }
          // Clear from seenSaleIds when no longer on sale so we re-notify next time
          wishlist.forEach(w => { if ((prices[w.appId]?.discount ?? 0) < 5) seenSaleIds.delete(w.appId) })
          store.set('seenSaleIds', [...seenSaleIds])
        }
      }
    } catch (e) {
      console.error('[wishlist-poll]', e.message)
    }
  }

  // 4. Fetch deals and push everything to renderer
  const deals = await fetchITADDeals(itadKey)

  cachedData = {
    freeGames:    [...ef, ...sf, ...gp],
    freeWeekends: sw,
    deals,
    wishlist:     wishlist.map(w => ({ ...w, priceInfo: prices[w.appId] ?? null })),
    lastChecked:  Date.now(),
  }

  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('data-update', cachedData)
  }
}

// ─── Poll scheduler ───────────────────────────────────────────────────────────

function startPolling () {
  if (pollTimer) clearInterval(pollTimer)
  const minutes = store.get('settings', DEFAULT_SETTINGS).checkInterval ?? 60
  const ms      = minutes * 60 * 1000
  poll()
  pollTimer = setInterval(poll, ms)
}

// ─── Window ───────────────────────────────────────────────────────────────────

function createWindow () {
  mainWindow = new BrowserWindow({
    width: 980, height: 660, minWidth: 760, minHeight: 540,
    frame: false,
    backgroundColor: '#0e1117',
    webPreferences: {
      preload:          path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration:  false,
    },
  })
  mainWindow.loadFile(path.join(__dirname, 'renderer', 'index.html'))
  mainWindow.on('close', e => {
    if (!app.isQuitting) { e.preventDefault(); mainWindow.hide() }
  })
}

// ─── Tray ─────────────────────────────────────────────────────────────────────

function createTray () {
  const icon = nativeImage
    .createFromPath(path.join(__dirname, 'assets', 'tray.png'))
    .resize({ width: 16, height: 16 })
  tray = new Tray(icon)
  tray.setToolTip('DealDrop')
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: 'Open DealDrop', click: () => { mainWindow.show(); mainWindow.focus() } },
    { label: 'Check now',     click: poll },
    { type: 'separator' },
    { label: 'Quit',          click: () => { app.isQuitting = true; app.quit() } },
  ]))
  tray.on('click', () => mainWindow.isVisible() ? mainWindow.hide() : mainWindow.show())
}

// ─── IPC handlers ─────────────────────────────────────────────────────────────

ipcMain.handle('get-settings',       ()      => store.get('settings',    DEFAULT_SETTINGS))
ipcMain.handle('save-settings',      (_, s)  => { store.set('settings',  s); startPolling() })
ipcMain.handle('get-itad-key',       ()      => store.get('itadApiKey',  ''))
ipcMain.handle('save-itad-key',      (_, k)  => store.set('itadApiKey',  k))
ipcMain.handle('get-steam-api-key',  ()      => store.get('steamApiKey', ''))
ipcMain.handle('save-steam-api-key', (_, k)  => store.set('steamApiKey', k))
ipcMain.handle('check-now',          ()      => poll())
ipcMain.handle('window-minimize',    ()      => mainWindow?.minimize())
ipcMain.handle('window-close',       ()      => mainWindow?.hide())
ipcMain.handle('open-url',           (_, u)  => shell.openExternal(u))
ipcMain.handle('open-steam',         (_, id) => shell.openExternal(`steam://store/${id}`))
ipcMain.handle('get-steam-user',     ()      => store.get('steamProfile', null))

ipcMain.handle('fetch-data', async () => {
  // Return cache immediately if available — poll() will push fresh data shortly
  if (cachedData) return cachedData

  const itadKey = store.get('itadApiKey', '')
  const steamId = store.get('steamId',    null)

  const [ef, sf, sw, gp, deals] = await Promise.all([
    fetchEpicFreeGames(),
    fetchSteamFeaturedFree(),
    fetchSteamFreeWeekends(),
    fetchGamerPowerGiveaways(),
    fetchITADDeals(itadKey),
  ])

  let wishlist = [], prices = {}
  if (steamId) {
    wishlist = await fetchWishlist(steamId)
    if (wishlist.length) prices = await checkPrices(wishlist.map(w => w.appId))
  }

  cachedData = {
    freeGames:    [...ef, ...sf, ...gp],
    freeWeekends: sw,
    deals,
    wishlist:     wishlist.map(w => ({ ...w, priceInfo: prices[w.appId] ?? null })),
    lastChecked:  Date.now(),
  }
  return cachedData
})

ipcMain.handle('steam-login', async () => {
  try {
    const steamId = await steamLogin()
    store.set('steamId', steamId)
    const profile = await fetchSteamProfile(steamId)
    store.set('steamProfile', profile)
    // Reset sale tracking so we get fresh notifications for this account
    seenSaleIds = new Set()
    store.set('seenSaleIds', [])
    // Invalidate cache so next fetch-data call gets fresh wishlist
    cachedData = null
    return { ok: true, profile }
  } catch (e) {
    console.error('[steam-login]', e.message)
    return { ok: false, error: e.message }
  }
})

ipcMain.handle('steam-logout', () => {
  store.delete('steamId')
  store.delete('steamProfile')
  seenSaleIds = new Set()
  store.set('seenSaleIds', [])
  cachedData  = null
  return { ok: true }
})

// Allow renderer to trigger a profile refresh (e.g. after saving an API key)
ipcMain.handle('refresh-profile', async () => {
  const steamId = store.get('steamId', null)
  if (!steamId) return null
  const profile = await fetchSteamProfile(steamId)
  store.set('steamProfile', profile)
  return profile
})

// ─── Boot ─────────────────────────────────────────────────────────────────────

app.whenReady().then(() => {
  createWindow()
  createTray()
  startPolling()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => { /* keep running in tray */ })
app.on('before-quit',       () => { app.isQuitting = true })
