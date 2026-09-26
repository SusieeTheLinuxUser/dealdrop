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

// Merge over defaults so settings saved by older app versions pick up new keys
function getSettings () {
  return { ...DEFAULT_SETTINGS, ...store.get('settings', {}) }
}

let mainWindow  = null
let tray        = null
let pollTimer   = null
let lastSaleKey = store.get('lastSaleKey', null)
let seenFreeIds = new Set(store.get('seenFreeIds', []))
let seenSaleIds = new Set(store.get('seenSaleIds', []))
let seenDealIds = new Set(store.get('seenDealIds', []))

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

// Several detectors read featuredcategories each poll — share one request
let fdcCache = { ts: 0, data: null }
async function fetchFeaturedcategories () {
  if (fdcCache.data && Date.now() - fdcCache.ts < 60_000) return fdcCache.data
  const data = await get('https://store.steampowered.com/api/featuredcategories/?cc=us&l=en')
  fdcCache = { ts: Date.now(), data }
  return data
}

// ─── Steam OpenID auth ────────────────────────────────────────────────────────

let pendingLogin = null                      // active login's callback dispatcher
const hookedSessions = new WeakSet()         // interceptors registered exactly once

function steamLogin () {
  return new Promise((resolve, reject) => {
    // Steam requires return_to and realm to be real resolvable HTTP(S) URLs.
    // We use http://localhost and intercept the request before it loads.
    const CALLBACK = 'http://localhost/steamcallback'

    const params = new URLSearchParams({
      'openid.mode':       'checkid_setup',
      'openid.ns':         'http://specs.openid.net/auth/2.0',
      'openid.identity':   'http://specs.openid.net/auth/2.0/identifier_select',
      'openid.claimed_id': 'http://specs.openid.net/auth/2.0/identifier_select',
      'openid.return_to':  CALLBACK,
      'openid.realm':      'http://localhost/',
    })

    const win = new BrowserWindow({
      width: 820, height: 640,
      parent: mainWindow, modal: true,
      title: 'Sign in through Steam',
      backgroundColor: '#1b2838',
      // Dedicated session so login cookies are kept (faster re-login) and the
      // localhost request interception below can never touch the rest of the app
      webPreferences: {
        nodeIntegration: false, contextIsolation: true,
        partition: 'steam-login',
      },
    })
    win.setMenu(null)

    let settled = false
    const settle = (fn) => {
      if (settled) return
      settled = true
      pendingLogin = null
      try { win.destroy() } catch {}
      fn()
    }

    const extractId = (rawUrl) => {
      try {
        const u         = new URL(rawUrl)
        const claimedId = u.searchParams.get('openid.claimed_id') ?? ''
        const m         = claimedId.match(/\/openid\/id\/(\d{17,})$/)
        return m ? m[1] : null
      } catch { return null }
    }

    // Primary: webRequest fires before the window tries to load localhost —
    // we cancel the navigation and read the Steam ID from query params.
    // The handler is registered once per session (see hookedSessions) and
    // dispatches through pendingLogin so repeated logins never stack handlers.
    pendingLogin = (url) => {
      const id = extractId(url)
      if (id) settle(() => resolve(id))
      else    settle(() => reject(new Error('Steam callback missing claimed_id')))
    }
    const session = win.webContents.session
    if (!hookedSessions.has(session)) {
      hookedSessions.add(session)
      session.webRequest.onBeforeRequest(
        { urls: ['http://localhost/steamcallback*'] },
        (details, callback) => {
          callback({ cancel: true })
          pendingLogin?.(details.url)
        }
      )
    }

    // Fallback: will-navigate fires before navigation commits
    win.webContents.on('will-navigate', (e, url) => {
      if (!url.startsWith(CALLBACK)) return
      e.preventDefault()
      const id = extractId(url)
      if (id) settle(() => resolve(id))
      else    settle(() => reject(new Error('Steam callback missing claimed_id')))
    })

    // Belt-and-suspenders: did-navigate fires after navigation
    win.webContents.on('did-navigate', (_, url) => {
      if (!url.startsWith(CALLBACK)) return
      const id = extractId(url)
      if (id) settle(() => resolve(id))
      else    settle(() => reject(new Error('Steam callback missing claimed_id')))
    })

    win.on('closed', () => settle(() => reject(new Error('Cancelled'))))
    win.loadURL(`https://steamcommunity.com/openid/login?${params}`)
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
// Strategy:
//   1. IWishlistService/GetWishlist/v1  — works WITHOUT any API key, fast,
//      no pagination. Returns appids + priority only (no names).
//   2. wishlistdata paginated endpoint  — also no key, returns names directly.
//
// Always returns a flat array so callers don't need to unwrap a shape object.
// Names from IWishlistService are null and get filled in by checkPrices below.

async function fetchWishlist (steamId) {
  // 1. IWishlistService — no key, single request, any public wishlist
  try {
    const data  = await get(
      `https://api.steampowered.com/IWishlistService/GetWishlist/v1?steamid=${steamId}`
    )
    const items = data?.response?.items ?? []
    if (items.length > 0) {
      console.log(`[wishlist] IWishlistService: ${items.length} items`)
      return items
        .map(i => ({
          appId:    String(i.appid),
          name:     null,            // filled in by checkPrices (basic filter)
          capsule:  true,
          priority: i.priority ?? 999,
        }))
        .sort((a, b) => a.priority - b.priority)
    }
  } catch (e) {
    console.warn('[wishlist] IWishlistService failed, trying wishlistdata:', e.message)
  }

  // 2. Paginated wishlistdata fallback (public profiles only, includes names)
  const items = []
  let page = 0

  while (true) {
    try {
      const res = await fetch(
        `https://store.steampowered.com/wishlist/profiles/${steamId}/wishlistdata/?p=${page}`,
        {
          headers: {
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36',
            'Accept':     'application/json, text/plain, */*',
            'Referer':    `https://store.steampowered.com/wishlist/profiles/${steamId}/`,
          },
        }
      )
      if (!res.ok) break
      const txt = (await res.text()).trim()
      if (!txt || txt.startsWith('<')) break   // HTML = private or age-gated

      const data = JSON.parse(txt)
      if (!data || typeof data !== 'object' || Object.keys(data).length === 0) break
      if (data.success === 2 || data.rwgrsn === -2) {
        console.warn('[wishlist] Wishlist is private or does not exist')
        break
      }

      for (const [appId, info] of Object.entries(data)) {
        if (isNaN(Number(appId))) continue
        items.push({
          appId:    appId,
          name:     info.name ?? null,
          capsule:  !!info.capsule,
          priority: info.priority ?? 999,
        })
      }
      page++
      if (Object.keys(data).length < 100) break
    } catch (e) {
      console.error('[wishlist] page fetch failed:', e.message)
      break
    }
  }

  return items.sort((a, b) => a.priority - b.priority)
}

// Distinguish "wishlist is empty" from "we can't see it" — the XML profile
// exposes privacyState without needing an API key.
async function isProfilePrivate (steamId) {
  try {
    const xml = await getText(`https://steamcommunity.com/profiles/${steamId}/?xml=1`)
    return /<privacyState>\s*private\s*<\/privacyState>/i.test(xml)
  } catch (e) {
    console.warn('[wishlist] privacy check failed:', e.message)
    return false
  }
}

// Shared by the poll and the renderer's first fetch so both produce the same
// shape: flat wishlist array (with priceInfo attached) + privacy flag.
async function loadWishlistData (steamId) {
  const wishlist = await fetchWishlist(steamId)
  if (!wishlist.length) return { items: [], isPrivate: await isProfilePrivate(steamId) }

  const prices = await checkPrices(wishlist.map(w => w.appId))
  const items = wishlist.map(w => ({
    ...w,
    name:      w.name ?? prices[w.appId]?.name ?? `App ${w.appId}`,
    priceInfo: prices[w.appId] ?? null,
  }))
  return { items, isPrivate: false }
}

// ─── Price checks ─────────────────────────────────────────────────────────────
// Steam's appdetails endpoint only accepts ONE appid per request these days —
// comma-separated ids silently return `null`. So we fetch individually, space
// the requests out, and cache results between polls.

const PRICE_CACHE_MAX_AGE = 7 * 24 * 60 * 60 * 1000 // drop entries unused for a week

function readPriceCache () {
  const cache  = store.get('priceCache', {})
  const cutoff = Date.now() - PRICE_CACHE_MAX_AGE
  let dirty = false
  for (const [id, e] of Object.entries(cache)) {
    if (!e || typeof e !== 'object' || e.ts < cutoff) { delete cache[id]; dirty = true }
  }
  if (dirty) store.set('priceCache', cache)
  return cache
}

// The response key can differ from the requested id (appids=620 answers with
// "323180"), so fall back to the single entry in the response.
async function fetchAppDetails (appId) {
  const res = await get(
    `https://store.steampowered.com/api/appdetails?appids=${encodeURIComponent(appId)}&filters=basic,price_overview&cc=us`
  )
  if (!res || typeof res !== 'object') return null
  const info = res[String(appId)] ?? Object.values(res)[0]
  return info?.success ? (info.data ?? null) : null
}

async function checkPrices (appIds) {
  if (!appIds.length) return {}
  const ttl     = Math.max(getSettings().checkInterval ?? 60, 15) * 60_000
  const now     = Date.now()
  const cache   = readPriceCache()
  const results = {}
  let changed   = false

  const toResult = e => ({
    name:      e.name      ?? null,
    discount:  e.discount  ?? 0,
    final:     e.final     ?? 0,
    initial:   e.initial   ?? 0,
    formatted: e.formatted ?? null,
  })

  for (const id of appIds) {
    const cached = cache[id]
    if (cached && now - cached.ts < ttl) {
      results[id] = toResult(cached)
      continue
    }

    let data = null
    let failed = false
    for (let attempt = 0; attempt < 2 && !data; attempt++) {
      try {
        data = await fetchAppDetails(id)
      } catch (e) {
        const retriable = /HTTP (429|5\d\d)/.test(e.message)
        if (attempt === 0 && retriable) { await sleep(1500); continue }
        failed = true
        console.warn(`[prices] ${id} failed: ${e.message}`)
      }
    }

    if (data) {
      const p = data.price_overview
      const entry = {
        ts: now,
        name:      data.name ?? null,
        discount:  p?.discount_percent ?? 0,
        final:     (p?.final   ?? 0) / 100,
        initial:   (p?.initial ?? 0) / 100,
        formatted: p?.final_formatted ?? null,
      }
      cache[id] = entry
      results[id] = toResult(entry)
      changed = true
    } else if (failed && cached) {
      results[id] = toResult(cached)   // stale beats missing
    } else if (!failed) {
      // Definitively no data (delisted, etc.) — stamp it so we stop re-asking
      cache[id] = { ts: now, name: null, discount: 0, final: 0, initial: 0, formatted: null }
      results[id] = toResult(cache[id])
      changed = true
    }

    await sleep(350)
  }

  if (changed) store.set('priceCache', cache)
  return results
}

async function fetchCheapSharkStores () {
  try {
    const list = await get('https://www.cheapshark.com/api/1.0/stores')
    if (!Array.isArray(list)) return {}
    return list.reduce((acc, s) => {
      acc[String(s.storeID)] = s.storeName ?? `Store ${s.storeID}`
      return acc
    }, {})
  } catch (e) {
    console.warn('[cheapshark-stores]', e.message)
    return {}
  }
}

async function fetchWishlistGameDeals (appId, gameName) {
  const result = {
    appId,
    gameName,
    steam: null,
    cheapest: null,
    checkedAt: Date.now(),
  }

  try {
    const data  = await fetchAppDetails(appId)
    const price = data?.price_overview ?? null
    if (price) {
      result.steam = {
        onSale: (price.discount_percent ?? 0) > 0,
        discount: price.discount_percent ?? 0,
        final: (price.final ?? 0) / 100,
        initial: (price.initial ?? 0) / 100,
        formatted: price.final_formatted ?? null,
      }
    }
  } catch (e) {
    console.warn('[wishlist-game-steam]', e.message)
  }

  try {
    const [stores, deals] = await Promise.all([
      fetchCheapSharkStores(),
      get(`https://www.cheapshark.com/api/1.0/deals?steamAppID=${encodeURIComponent(appId)}&pageSize=15`),
    ])
    if (Array.isArray(deals) && deals.length > 0) {
      const sorted = deals
        .filter(d => Number(d.salePrice) > 0)
        .sort((a, b) => Number(a.salePrice) - Number(b.salePrice))
      const best = sorted[0] ?? deals[0]
      if (best) {
        result.cheapest = {
          store: stores[String(best.storeID)] ?? `Store ${best.storeID}`,
          storeId: String(best.storeID),
          price: Number(best.salePrice),
          retailPrice: Number(best.normalPrice),
          savingsPercent: Math.round(Number(best.savings) || 0),
          dealId: best.dealID ?? null,
          url: best.dealID ? `https://www.cheapshark.com/redirect?dealID=${encodeURIComponent(best.dealID)}` : null,
        }
      }
    }
  } catch (e) {
    console.warn('[wishlist-game-cheapest]', e.message)
  }

  return result
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
        const slug  = g.productSlug
                    ?? g.catalogNs?.mappings?.[0]?.pageSlug
                    ?? g.offerMappings?.[0]?.pageSlug
                    ?? g.urlSlug
                    ?? ''
        return {
          id:      'epic-' + g.id,
          title:   g.title,
          source:  'epic',
          url:     slug ? `https://store.epicgames.com/p/${slug}` : 'https://store.epicgames.com/free-games',
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
    const fdc  = await fetchFeaturedcategories()
    const out  = []
    const seen = new Set()

    // Primary: the "Free Weekend" spotlight banner links straight to the game.
    for (const s of getSpotlights(fdc)) {
      if (!/free weekend/i.test(s.name ?? '')) continue
      const appId = (s.url ?? '').match(/\/app\/(\d+)/)?.[1]
      if (!appId || seen.has(appId)) continue
      seen.add(appId)

      let title = 'Free weekend game'
      let image = null
      try {
        const d = await fetchAppDetails(appId)
        if (d) { title = d.name ?? title; image = d.header_image ?? null }
      } catch (e) {
        console.warn('[steam-weekends] details failed:', e.message)
      }
      out.push({
        id:     'steam-fw-' + appId,
        appId,
        title,
        source: 'steam',
        url:    `https://store.steampowered.com/app/${appId}`,
        image,
        type:   'weekend',
      })
    }

    // Legacy fallback: coming_soon items at 100% discount (older API shape)
    for (const i of (fdc?.coming_soon?.items ?? [])) {
      if (i.discount_percent !== 100 || seen.has(String(i.id))) continue
      seen.add(String(i.id))
      out.push({
        id:     'steam-fw-' + i.id,
        appId:  String(i.id),
        title:  i.name,
        source: 'steam',
        url:    `https://store.steampowered.com/app/${i.id}`,
        image:  i.large_capsule_image ?? null,
        type:   'weekend',
      })
    }
    return out
  } catch (e) {
    console.error('[steam-weekends]', e.message)
    return []
  }
}

async function fetchGamerPowerGiveaways () {
  try {
    const data = await get('https://www.gamerpower.com/api/giveaways?platform=pc')
    if (!Array.isArray(data)) return []
    return data
      .filter(g => {
        // Drop giveaways whose end date has passed (N/A = open-ended)
        if (!g.end_date || g.end_date === 'N/A') return true
        const end = new Date(g.end_date).getTime()
        return isNaN(end) || end > Date.now()
      })
      .map(g => ({
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
// Steam replaced the old top-level `spotlight` category with numbered
// "cat_spotlight" entries — e.g. one banner reading "Free Weekend" linking to
// the game, another reading "FRANCHISE SALE". Seasonal sales show up here too.

function getSpotlights (fdc) {
  return Object.values(fdc ?? {})
    .filter(c => c?.id === 'cat_spotlight' && Array.isArray(c.items))
    .flatMap(c => c.items)
}

// Matches "Steam Summer Sale", "Lunar New Year Sale", … but NOT "FRANCHISE SALE"
const MAJOR_SALE_RE =
  /(summer|winter|autumn|spring|lunar|halloween|seasonal).*sale|sale.*(summer|winter|autumn|spring|lunar|halloween|seasonal)/i

async function detectSteamSale () {
  try {
    const fdc        = await fetchFeaturedcategories()
    const specials   = fdc?.specials?.items ?? []
    const heavyCount = specials.filter(i => (i.discount_percent ?? 0) >= 50).length
    const banner = getSpotlights(fdc).find(s =>
      MAJOR_SALE_RE.test(s.name ?? '') ||
      /\/sale\/[^/]*(summer|winter|autumn|spring|lunar|halloween)/i.test(s.url ?? '')
    )
    const isSaleOn = !!banner || heavyCount >= 8
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

// Callers (timer, tray, IPC, renderer) all share one in-flight poll so a slow
// run can't be started twice, and errors can't become unhandled rejections.
let pollPromise = null
function poll () {
  if (!pollPromise) {
    pollPromise = runPoll()
      .catch(e => console.error('[poll]', e))
      .finally(() => { pollPromise = null })
  }
  return pollPromise
}

async function runPoll () {
  console.log('[poll] checking…')
  const settings = getSettings()
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
    checkNew(sw,                    'weekend', 'notifyFreeWeekend')
  }

  // 3. Wishlist sale notifications
  // Start from the last good snapshot so a transient fetch error doesn't wipe
  // the wishlist the user is looking at
  let wl = {
    items:     cachedData?.wishlist ?? [],
    isPrivate: cachedData?.wishlistPrivate ?? false,
  }
  if (steamId) {
    try {
      wl = await loadWishlistData(steamId)
    } catch (e) {
      console.error('[wishlist-poll]', e.message)
    }
  }

  if (settings.notifyWishlistSale) {
    const onSale = wl.items.filter(w => (w.priceInfo?.discount ?? 0) >= 20)
    const fresh  = onSale.filter(w => !seenSaleIds.has(w.appId))
    if (fresh.length) {
      fresh.forEach(w => seenSaleIds.add(w.appId))
      store.set('seenSaleIds', [...seenSaleIds])
      if (fresh.length === 1) {
        const p = fresh[0].priceInfo
        notify(`🏷️ ${fresh[0].name} is on sale!`, `${p.discount}% off — now ${p.formatted} on Steam`)
      } else {
        notify(
          `${fresh.length} wishlist games on sale`,
          fresh.slice(0, 4).map(w => w.name).join(', ') + (fresh.length > 4 ? '…' : '')
        )
      }
    }
    // Clear from seenSaleIds when no longer on sale so we re-notify next time
    wl.items.forEach(w => { if ((w.priceInfo?.discount ?? 0) < 5) seenSaleIds.delete(w.appId) })
    store.set('seenSaleIds', [...seenSaleIds])
  }

  // 4. Deals (and optional big-deal notifications) + push everything to renderer
  const deals = await fetchITADDeals(itadKey)

  if (settings.notifyBigDeal && deals.length) {
    const big   = deals.filter(d => (d.cut ?? 0) >= 75)
    const fresh = big.filter(d => d.id && !seenDealIds.has(d.id))
    if (fresh.length) {
      fresh.forEach(d => seenDealIds.add(d.id))
      if (fresh.length === 1) {
        const d = fresh[0]
        notify(`🔥 ${d.cut}% off: ${d.title}`, `${d.shop} — $${(d.price ?? 0).toFixed(2)}`)
      } else {
        notify(
          `${fresh.length} deals over 75% off`,
          fresh.slice(0, 4).map(d => d.title).join(', ') + (fresh.length > 4 ? '…' : '')
        )
      }
    }
    // Hysteresis: forget a deal once it drops back below 60% so a flickering
    // discount doesn't re-notify, but a re-deepened one still does
    deals.forEach(d => { if ((d.cut ?? 0) < 60) seenDealIds.delete(d.id) })
    store.set('seenDealIds', [...seenDealIds])
  }

  cachedData = {
    freeGames:       [...ef, ...sf, ...gp],
    freeWeekends:    sw,
    deals,
    wishlist:        wl.items,
    wishlistPrivate: wl.isPrivate,
    lastChecked:     Date.now(),
  }

  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('data-update', cachedData)
  }
  return cachedData
}

// ─── Poll scheduler ───────────────────────────────────────────────────────────

function startPolling () {
  if (pollTimer) clearInterval(pollTimer)
  const minutes = getSettings().checkInterval ?? 60
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

ipcMain.handle('get-settings',       ()      => getSettings())
ipcMain.handle('save-settings',      (_, s)  => {
  store.set('settings', { ...getSettings(), ...s })
  startPolling()
})
ipcMain.handle('get-itad-key',       ()      => store.get('itadApiKey',  ''))
ipcMain.handle('save-itad-key',      (_, k)  => store.set('itadApiKey',  k))
ipcMain.handle('get-steam-api-key',  ()      => store.get('steamApiKey', ''))
ipcMain.handle('save-steam-api-key', (_, k)  => store.set('steamApiKey', k))
ipcMain.handle('check-now',          ()      => poll())
ipcMain.handle('window-minimize',    ()      => mainWindow?.minimize())
ipcMain.handle('window-close',       ()      => mainWindow?.hide())

// Only let the renderer open URLs we'd actually want opened — never file:// or
// arbitrary custom schemes pulled from API data
ipcMain.handle('open-url', (_, u) => {
  try {
    const url = new URL(String(u))
    if (!['https:', 'http:', 'steam:'].includes(url.protocol)) return false
    return shell.openExternal(url.href).then(() => true, () => false)
  } catch {
    return false
  }
})
ipcMain.handle('open-steam', (_, id) => {
  const appId = String(id ?? '')
  if (!/^\d+$/.test(appId)) return false
  return shell.openExternal(`steam://store/${appId}`).then(() => true, () => false)
})
ipcMain.handle('get-steam-user',     ()      => store.get('steamProfile', null))

ipcMain.handle('fetch-data', async () => {
  // Return cache immediately if available — poll() pushes fresh data shortly
  // afterwards. On a cold start, share the poll that's already running (or
  // start one) instead of duplicating every one of its requests.
  if (cachedData) return cachedData
  await poll()
  return cachedData ?? {}
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

ipcMain.handle('fetch-wishlist-game-deals', async (_, payload) => {
  const appId = String(payload?.appId ?? '').trim()
  const gameName = String(payload?.name ?? '').trim()
  if (!appId) return { ok: false, error: 'Missing appId' }
  try {
    const data = await fetchWishlistGameDeals(appId, gameName || `App ${appId}`)
    return { ok: true, data }
  } catch (e) {
    console.error('[wishlist-game-deals]', e.message)
    return { ok: false, error: e.message }
  }
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