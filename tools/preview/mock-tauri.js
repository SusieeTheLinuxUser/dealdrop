'use strict';

// Development-only Tauri bridge stub. All data stays in this page's memory.
(() => {
  const flags = new URLSearchParams(location.search)
  const signedIn = flags.get('accounts') === '1'
  const empty = flags.get('empty') === '1'
  const lastChecked = Date.now() - (flags.get('stale') === '1' ? 3 * 60 * 60 * 1000 : 5 * 60 * 1000)
  let settings = {
    notifyMajorSale: true, notifyFreeGame: true, notifyFreeWeekend: true,
    notifyWishlistSale: true, notifyBigDeal: false, checkInterval: 60,
    theme: 'midnight', baseTheme: 'midnight', lastSaleTheme: '',
    rounded: false, effects: true, launcherGog: 'auto', launcherEpic: 'auto',
    onboarded: flags.get('onboarded') === '1',
  }
  let accounts = {
    steam: signedIn ? { name: 'PlayerOne', avatar: null, profileUrl: 'https://steamcommunity.com/' } : null,
    gog: signedIn ? { name: 'PlayerOne', avatar: null, profileUrl: 'https://www.gog.com/u/' } : null,
    epic: signedIn ? { name: 'PlayerOne', avatar: null, profileUrl: 'https://www.epicgames.com/account/personal' } : null,
  }
  const deal = (id, title, shop, cut, price, regular, appId) => ({
    id, title, shop, url: `https://store.steampowered.com/app/${appId}`, image: null,
    cut, price, regular, appId,
  })
  const sample = {
    freeGames: empty ? [] : [
      { id: 'epic-sample-free', title: 'The Long Way Home', source: 'epic', url: 'https://store.epicgames.com/', image: null, appId: null, endDate: null, type: 'giveaway' },
      { id: 'steam-440', title: 'Team Fortress 2', source: 'steam', url: 'https://store.steampowered.com/app/440', image: null, appId: '440', endDate: null, type: 'free' },
      { id: 'steam-ftk-4841320', title: 'Squad Weapon Skins - Recon Team Pack (DLC)', source: 'steam', url: 'https://store.steampowered.com/app/4841320', image: null, appId: '4841320', endDate: null, type: 'free' },
      { id: 'gog-123456', title: 'Classic Quest', source: 'gog', url: 'https://www.gog.com/game/classic_quest', image: null, appId: null, endDate: null, type: 'giveaway' },
      { id: 'other-demo', title: 'Community Game', source: 'other', url: 'https://example.invalid/game', image: null, appId: null, endDate: null, type: 'giveaway' },
    ],
    freeWeekends: empty ? [] : [
      { id: 'steam-fw-570', title: 'Dota 2 Weekend Trial', source: 'steam', url: 'https://store.steampowered.com/app/570', image: null, appId: '570', endDate: '2027-01-01', type: 'weekend' },
    ],
    sales: empty ? { steam: [], gog: [], epic: [] } : {
      steam: [deal('steam-sale-440', 'Team Fortress 2', 'Steam', 75, 2.49, 9.99, '440')],
      gog: [deal('gog-sale-123456', 'Classic Quest', 'GOG', 60, 3.99, 9.99, '123456')],
      epic: [deal('epic-sale-123', 'Skyward Bound', 'Epic Games', 40, 11.99, 19.99, '123')],
    },
    liveSale: flags.get('sale') === 'winter' ? { key: 'winter', name: 'Winter Sale', url: 'https://store.steampowered.com/specials' } : null,
    deals: empty ? [] : [
      deal('deal-440', 'Team Fortress 2', 'Steam', 75, 2.49, 9.99, '440'),
      deal('deal-123', 'Skyward Bound', 'GOG', 50, 14.99, 29.99, '123'),
      deal('deal-456', 'Garden Tactics', 'Epic Games', 30, 6.99, 9.99, '456'),
    ],
    dealsSource: 'CheapShark', dealsError: null,
    wishlist: empty || !signedIn ? [] : [
      { store: 'steam', id: 'steam-440', appId: '440', name: 'Team Fortress 2', image: null, url: 'https://store.steampowered.com/app/440', priceInfo: { discount: 50, final: 4.99, initial: 9.99, formatted: '$4.99' }, cheapest: { store: 'GOG', price: 3.99, url: 'https://www.gog.com/game/classic_quest' } },
      { store: 'gog', id: 'gog-123456', appId: null, name: 'Classic Quest', image: null, url: 'https://www.gog.com/game/classic_quest', priceInfo: { discount: 0, final: 9.99, initial: 9.99, formatted: '$9.99' }, cheapest: null },
      { store: 'epic', id: 'epic-123', appId: null, name: 'Skyward Bound', image: null, url: 'https://store.epicgames.com/', priceInfo: { discount: 20, final: 7.99, initial: 9.99, formatted: '$7.99' }, cheapest: null },
    ],
    wishlistPrivate: false,
    wishlistErrors: signedIn && flags.get('errors') === '1' ? { gog: 'Could not refresh wishlist' } : {},
    sourceErrors: flags.get('errors') === '1' ? ['Epic sales: HTTP 500', 'GOG giveaways: unavailable'] : [],
    lastChecked,
  }
  const emitters = new Set()
  const launcherOptions = {
    steam: [{ key: 'native', label: 'Steam' }],
    gog: [{ key: 'native', label: 'GOG Galaxy' }, { key: 'heroic', label: 'Heroic' }],
    epic: [{ key: 'native', label: 'Epic Games' }, { key: 'heroic', label: 'Heroic' }],
  }
  const launchers = () => Object.fromEntries(Object.entries(launcherOptions).map(([store, options]) => {
    const pref = store === 'gog' ? settings.launcherGog : store === 'epic' ? settings.launcherEpic : 'auto'
    const chosen = pref === 'browser' ? null : options.find(option => option.key === pref) || options[0] || null
    return [store, { options: clone(options), chosen: clone(chosen) }]
  }))
  const clone = value => JSON.parse(JSON.stringify(value))
  async function invoke (cmd, args = {}) {
    switch (cmd) {
      case 'get_settings': return clone(settings)
      case 'save_settings': settings = { ...settings, ...(args.settings || {}) }; return undefined
      case 'fetch_data':
      case 'check_now': sample.lastChecked = Date.now(); return clone(sample)
      case 'get_accounts': return clone(accounts)
      case 'launchers': return launchers()
      case 'steam_login': accounts.steam = { name: 'Preview Steam', avatar: null, profileUrl: 'https://steamcommunity.com/' }; return clone(accounts)
      case 'gog_login': accounts.gog = { name: 'Preview GOG', avatar: null, profileUrl: 'https://www.gog.com/u/' }; return clone(accounts)
      case 'epic_login': accounts.epic = { name: 'Preview Epic', avatar: null, profileUrl: 'https://www.epicgames.com/account/personal' }; return clone(accounts)
      case 'logout': accounts[args.store] = null; return clone(accounts)
      case 'wishlist_game_deals': return { appId: args.appId, gameName: args.name, steam: { onSale: true, discount: 50, final: 4.99, initial: 9.99, formatted: '$4.99' }, cheapest: { store: 'GOG', price: 3.99, url: 'https://www.gog.com/game/classic_quest', savingsPercent: 60, retailPrice: 9.99 }, checkedAt: Date.now() }
      case 'open_url':
      case 'open_in_app': console.info(`[preview] ${cmd}`, args); return true
      case 'window_minimize':
      case 'window_close': console.info(`[preview] ${cmd}`); return undefined
      default: throw new Error(`Preview bridge does not implement: ${cmd}`)
    }
  }
  window.__TAURI__ = {
    core: { invoke },
    event: { listen: async (_event, callback) => { emitters.add(callback); return () => emitters.delete(callback) } },
    window: { getCurrentWindow: () => ({ startResizeDragging: direction => console.info('[preview] resize', direction) }) },
    app: { getVersion: async () => '0.7.9' },
  }
  window.dealDropPreview = { flags, emit: payload => emitters.forEach(callback => callback({ payload })) }
})()
