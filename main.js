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

let mainWindow   = null
let tray         = null
let pollTimer    = null
let lastSaleKey  = store.get('lastSaleKey', null)
let seenFreeIds  = new Set(store.get('seenFreeIds', []))
let seenSaleIds  = new Set(store.get('seenSaleIds', []))

// ─── Fetch helper ─────────────────────────────────────────────────────────────

async function get (url) {
  const res = await fetch(url, {
    headers: {
      'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
      'Referer': 'https://store.steampowered.com/',
      'Accept': 'application/json, text/plain, */*'
    }
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
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
      width: 800, height: 620,
      parent: mainWindow, modal: true,
      title: 'Sign in through Steam',
      backgroundColor: '#1b2838',
      webPreferences: { nodeIntegration: false, contextIsolation: true },
    })

    win.loadURL(`https://steamcommunity.com/openid/login?${params}`)

    const check = (url) => {
      if (!url.startsWith(RETURN_URL)) return
      const match = url.match(/openid\.identity=https?%3A%2F%2Fsteamcommunity\.com%2Fopenid%2Fid%2F(\d+)/)
      if (match) { win.close(); resolve(match[1]) }
      else        { win.close(); reject(new Error('Could not extract Steam ID')) }
    }

    win.webContents.on('will-redirect', (_, url) => check(url))
    win.webContents.on('did-navigate',  (_, url) => check(url))
    win.on('closed', () => reject(new Error('Cancelled')))
  })
}

// ─── Steam profile & wishlist ─────────────────────────────────────────────────

async function fetchSteamProfile (steamId) {
  const apiKey = store.get('steamApiKey', '')
  if (apiKey) {
    try {
      const data = await get(`https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key=${apiKey}&steamids=${steamId}`)
      const p    = data?.response?.players?.[0]
      if (p) return { steamId, name: p.personaname, avatar: p.avatarmedium }
    } catch {}
  }
  return { steamId, name: `Steam user ···${steamId.slice(-4)}`, avatar: null }
}

async function fetchWishlist (steamId) {
  const items = []
  let page = 0
  while (true) {
    try {
      const data = await get(`https://store.steampowered.com/wishlist/profiles/${steamId}/wishlistdata/?p=${page}`)
      if (!data || Object.keys(data).length === 0) break
      for (const [appId, info] of Object.entries(data)) {
        items.push({ appId, name: info.name, capsule: info.capsule, priority: info.priority ?? 999 })
      }
      page++
      if (Object.keys(data).length < 100) break
    } catch { break }
  }
  return items.sort((a, b) => a.priority - b.priority)
}

async function checkPrices (appIds) {
  const results = {}
  const BATCH   = 20
  for (let i = 0; i < appIds.length; i += BATCH) {
    const chunk = appIds.slice(i, i + BATCH).join(',')
    try {
      const data = await get(`https://store.steampowered.com/api/appdetails?appids=${chunk}&filters=price_overview&cc=us`)
      for (const [id, info] of Object.entries(data)) {
        if (info.success && info.data?.price_overview) {
          const p     = info.data.price_overview
          results[id] = { discount: p.discount_percent, final: p.final / 100, initial: p.initial / 100, formatted: p.final_formatted }
        }
      }
    } catch {}
    await new Promise(r => setTimeout(r, 250))
  }
  return results
}

// ─── Other data sources ───────────────────────────────────────────────────────

async function fetchEpicFreeGames () {
  try {
    const data  = await get('https://store-site-backend-static.ak.epicgames.com/freeGamesPromotions?locale=en-US&country=US&allowCountries=US')
    const elems = data?.data?.Catalog?.searchStore?.elements ?? []
    return elems
      .filter(g => g.promotions?.promotionalOffers?.[0]?.promotionalOffers?.[0]?.discountSetting?.discountPercentage === 0)
      .map(g => {
        const offer = g.promotions.promotionalOffers[0].promotionalOffers[0]
        const slug  = g.productSlug ?? g.catalogNs?.mappings?.[0]?.pageSlug ?? ''
        return { id: 'epic-'+g.id, title: g.title, source: 'epic', url: `https://store.epicgames.com/p/${slug}`, endDate: offer.endDate, image: g.keyImages?.find(i => i.type==='Thumbnail')?.url ?? null, type: 'free' }
      })
  } catch { return [] }
}

async function fetchSteamFeatured () {
  try {
    const data  = await get('https://store.steampowered.com/api/featured/?cc=us&l=en')
    const items = [...(data.large_capsules ?? []), ...(data.featured_win ?? [])]
    return items.filter(i => i.final_price === 0 && (i.original_price ?? 0) > 0).map(i => ({
      id: 'steam-'+i.id, appId: String(i.id), title: i.name, source: 'steam',
      url: `https://store.steampowered.com/app/${i.id}`, image: i.large_capsule_image ?? null, type: 'free'
    }))
  } catch { return [] }
}

async function fetchSteamFreeWeekends () {
  try {
    const data = await get('https://store.steampowered.com/api/featuredcategories/?cc=us&l=en')
    return (data?.coming_soon?.items ?? []).filter(i => i.discount_percent === 100).map(i => ({
      id: 'steam-fw-'+i.id, appId: String(i.id), title: i.name, source: 'steam',
      url: `https://store.steampowered.com/app/${i.id}`, image: i.large_capsule_image ?? null, type: 'weekend'
    }))
  } catch { return [] }
}

async function fetchGamerPowerGiveaways () {
  try {
    const data = await get('https://www.gamerpower.com/api/giveaways?platform=PC')
    return (data ?? []).map(g => {
      const isSteam = g.platform?.toLowerCase().includes('steam')
      const isEpic = g.platform?.toLowerCase().includes('epic')
      return {
        id: 'gp-' + g.id,
        title: g.title,
        source: isSteam ? 'steam' : isEpic ? 'epic' : 'other',
        appId: isSteam ? g.steam_appid ?? null : null,
        url: g.open_giveaway_url ?? g.open_giveaway ?? '#',
        endDate: g.end_date,
        image: g.thumbnail ?? null,
        type: 'free'
      }
    })
  } catch (e) {
    console.error('[GamerPower]', e.message)
    return []
  }
}

async function detectSteamSale () {
  try {
    const data       = await get('https://store.steampowered.com/api/featuredcategories/?cc=us&l=en')
    const specials   = data?.specials?.items ?? []
    const heavyCount = specials.filter(i => (i.discount_percent ?? 0) >= 50).length
    const spotlight  = data?.spotlight?.items ?? []
    const banner     = spotlight.find(i => /sale|fest|event/i.test(i.name ?? ''))
    const isSaleOn   = heavyCount >= 8 || !!banner
    return { isSaleOn, saleName: banner?.name ?? (isSaleOn ? 'Steam Sale' : null) }
  } catch { return { isSaleOn: false, saleName: null } }
}

async function fetchITADDeals (apiKey) {
  if (!apiKey) return []
  try {
    const data = await get(`https://api.isthereanydeal.com/deals/v2?key=${apiKey}&limit=25&sort=-cut`)
    return (data.list ?? []).map(d => ({
      id: d.id, title: d.title, cut: d.deal?.cut ?? 0, price: d.deal?.price?.amount ?? 0,
      regular: d.deal?.regular?.amount ?? 0, shop: d.deal?.shop?.name ?? 'unknown', url: d.deal?.url ?? '#'
    }))
  } catch { return [] }
}

// ─── Notification helper ──────────────────────────────────────────────────────

function notify (title, body) {
  if (!Notification.isSupported()) return
  const n = new Notification({ title, body })
  n.on('click', () => { mainWindow?.show(); mainWindow?.focus() })
  n.show()
}

// ─── Poll loop ────────────────────────────────────────────────────────────────

async function poll () {
  console.log('[poll] checking…')
  const settings = store.get('settings',   DEFAULT_SETTINGS)
  const itadKey  = store.get('itadApiKey', '')
  const steamId  = store.get('steamId',    null)

  // 1. Steam major sale
  if (settings.notifyMajorSale) {
    const { isSaleOn, saleName } = await detectSteamSale()
    const key = isSaleOn ? (saleName ?? 'sale') : null
    if (isSaleOn && key !== lastSaleKey) { lastSaleKey = key; store.set('lastSaleKey', key); notify('🎉 Steam sale started!', `${saleName} is live — go grab some deals.`) }
    else if (!isSaleOn) { lastSaleKey = null; store.set('lastSaleKey', null) }
  }

  // 2. Free games
if (settings.notifyFreeGame || settings.notifyFreeWeekend) {
  const [ef, sf, sw, gp] = await Promise.all([
    fetchEpicFreeGames(),
    fetchSteamFeatured(),
    fetchSteamFreeWeekends(),
    fetchGamerPowerGiveaways()
  ])
  
  const checkNew = (games, type, key) => {
    if (!settings[key]) return
    const fresh = games.filter(g => !seenFreeIds.has(g.id))
    if (!fresh.length) return
    fresh.forEach(g => seenFreeIds.add(g.id))
    store.set('seenFreeIds', [...seenFreeIds])
    if (fresh.length === 1) {
      const src = fresh[0].source?.toUpperCase() ?? 'STORE'
      notify(
        type === 'weekend' ? `Free weekend: ${fresh[0].title}` : `Free: ${fresh[0].title}`,
        `${fresh[0].title} is free on ${src}!`
      )
    } else {
      notify(
        `${fresh.length} free ${type === 'weekend' ? 'weekends' : 'games'} available`,
        fresh.map(g => g.title).join(', ')
      )
    }
  }
  checkNew(ef, 'free', 'notifyFreeGame')
  checkNew(sf, 'free', 'notifyFreeGame')
  checkNew(sw, 'weekend', 'notifyFreeWeekend')
  checkNew(gp, 'free', 'notifyFreeGame') // ← new: notify for GamerPower giveaways
}

  // 3. Wishlist sale check
  if (settings.notifyWishlistSale && steamId) {
    try {
      const wishlist = await fetchWishlist(steamId)
      if (wishlist.length) {
        const prices = await checkPrices(wishlist.map(w => w.appId))
        const onSale = wishlist.filter(w => (prices[w.appId]?.discount ?? 0) >= 20)
        const fresh  = onSale.filter(w => !seenSaleIds.has(w.appId))
        if (fresh.length) {
          fresh.forEach(w => seenSaleIds.add(w.appId))
          store.set('seenSaleIds', [...seenSaleIds])
          if (fresh.length === 1) { const p = prices[fresh[0].appId]; notify(`🏷️ ${fresh[0].name} is on sale!`, `${p.discount}% off — now ${p.formatted} on Steam`) }
          else notify(`${fresh.length} wishlist games on sale`, fresh.slice(0,4).map(w => w.name).join(', ') + (fresh.length > 4 ? '…' : ''))
        }
        wishlist.forEach(w => { if ((prices[w.appId]?.discount ?? 0) < 5) seenSaleIds.delete(w.appId) })
        store.set('seenSaleIds', [...seenSaleIds])
      }
    } catch (e) { console.error('[wishlist-poll]', e.message) }
  }

  // 4. Push data to renderer
if (mainWindow && !mainWindow.isDestroyed()) {
  try {
    const [ef, sf, sw, gp, deals] = await Promise.all([
      fetchEpicFreeGames(),
      fetchSteamFeatured(),
      fetchSteamFreeWeekends(),
      fetchGamerPowerGiveaways(),
      fetchITADDeals(itadKey)
    ])
    
    let wishlist = [], prices = {}
    if (steamId) {
      wishlist = await fetchWishlist(steamId)
      if (wishlist.length) prices = await checkPrices(wishlist.map(w => w.appId))
    }
    
    mainWindow.webContents.send('data-update', {
      freeGames: [...ef, ...sf, ...gp], // ← added ...gp here
      freeWeekends: sw,
      deals,
      wishlist: wishlist.map(w => ({ ...w, priceInfo: prices[w.appId] ?? null })),
      lastChecked: Date.now(),
    })
  } catch (e) { console.error('[push]', e.message) }
}
  
function startPolling () {
  if (pollTimer) clearInterval(pollTimer)
  const ms = (store.get('settings', DEFAULT_SETTINGS).checkInterval ?? 60) * 60 * 1000
  poll()
  pollTimer = setInterval(poll, ms)
}

// ─── Window & tray ────────────────────────────────────────────────────────────

function createWindow () {
  mainWindow = new BrowserWindow({
    width: 980, height: 660, minWidth: 760, minHeight: 540,
    frame: false, backgroundColor: '#0e1117',
    webPreferences: { preload: path.join(__dirname, 'preload.js'), contextIsolation: true, nodeIntegration: false },
  })
  mainWindow.loadFile(path.join(__dirname, 'renderer', 'index.html'))
  mainWindow.on('close', e => { if (!app.isQuitting) { e.preventDefault(); mainWindow.hide() } })
}

function createTray () {
  const icon = nativeImage.createFromPath(path.join(__dirname, 'assets', 'tray.png')).resize({ width: 16, height: 16 })
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

// ─── IPC ──────────────────────────────────────────────────────────────────────

ipcMain.handle('get-settings',      ()      => store.get('settings',    DEFAULT_SETTINGS))
ipcMain.handle('save-settings',     (_, s)  => { store.set('settings',  s); startPolling() })
ipcMain.handle('get-itad-key',      ()      => store.get('itadApiKey',  ''))
ipcMain.handle('save-itad-key',     (_, k)  => store.set('itadApiKey',  k))
ipcMain.handle('get-steam-api-key', ()      => store.get('steamApiKey', ''))
ipcMain.handle('save-steam-api-key',(_, k)  => store.set('steamApiKey', k))
ipcMain.handle('check-now',         ()      => poll())
ipcMain.handle('window-minimize',   ()      => mainWindow?.minimize())
ipcMain.handle('window-close',      ()      => mainWindow?.hide())
ipcMain.handle('open-url',          (_, u)  => shell.openExternal(u))
ipcMain.handle('open-steam',        (_, id) => shell.openExternal(`steam://store/${id}`))
ipcMain.handle('get-steam-user',    ()      => store.get('steamProfile', null))

ipcMain.handle('steam-login', async () => {
  try {
    const steamId = await steamLogin()
    store.set('steamId', steamId)
    const profile = await fetchSteamProfile(steamId)
    store.set('steamProfile', profile)
    seenSaleIds = new Set(); store.set('seenSaleIds', [])
    return { ok: true, profile }
  } catch (e) { return { ok: false, error: e.message } }
})

ipcMain.handle('steam-logout', () => {
  store.delete('steamId'); store.delete('steamProfile')
  seenSaleIds = new Set(); store.set('seenSaleIds', [])
  return { ok: true }
})

ipcMain.handle('fetch-data', async () => {
  const [ef, sf, sw, deals] = await Promise.all([fetchEpicFreeGames(), fetchSteamFeatured(), fetchSteamFreeWeekends(), fetchITADDeals(store.get('itadApiKey',''))])
  const steamId = store.get('steamId', null)
  let wishlist = [], prices = {}
  if (steamId) { wishlist = await fetchWishlist(steamId); if (wishlist.length) prices = await checkPrices(wishlist.map(w => w.appId)) }
  return { freeGames: [...ef, ...sf], freeWeekends: sw, deals, wishlist: wishlist.map(w => ({ ...w, priceInfo: prices[w.appId] ?? null })), lastChecked: Date.now() }
})

// ─── Boot ─────────────────────────────────────────────────────────────────────

app.whenReady().then(() => { createWindow(); createTray(); startPolling() })
app.on('window-all-closed', () => {})
app.on('before-quit', () => { app.isQuitting = true })
