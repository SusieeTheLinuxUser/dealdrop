'use strict';

// Development-only Tauri bridge stub. All data stays in this page's memory.
(() => {
  const flags = new URLSearchParams(location.search)
  const stress = flags.get('stress') === '1'
  const stressTitle = 'UnbrokenStressTitle' + 'X'.repeat(100)
  const stressAccount = 'PreviewAccount' + 'Y'.repeat(90)
  const accountFlag = flags.get('accounts') || ''
  const signedStores = new Set(stress || accountFlag === '1' ? ['steam', 'gog', 'epic'] : accountFlag.split(',').filter(store => ['steam', 'gog', 'epic'].includes(store)))
  const empty = flags.get('empty') === '1'
  const lastChecked = Date.now() - (flags.get('stale') === '1' ? 3 * 60 * 60 * 1000 : 5 * 60 * 1000)
  let settings = {
    notifyMajorSale: true, notifySaleSoon: true, notifyFreeGame: true, notifyFreeWeekend: true,
    notifyWishlistSale: true, wishlistMinCut: 20, notifyBigDeal: false, bigDealMinCut: 75, checkInterval: 60,
    theme: 'midnight', baseTheme: 'midnight', lastSaleTheme: '',
    rounded: false, effects: true, launcherGog: 'auto', launcherEpic: 'auto', showLoot: false,
    onboarded: flags.get('onboarded') === '1', hidden: [], priceTargets: {},
  }
  let autostart = false
  let accounts = {
    steam: signedStores.has('steam') ? { name: stress ? stressAccount : 'PlayerOne', avatar: null, profileUrl: 'https://steamcommunity.com/' } : null,
    gog: signedStores.has('gog') ? { name: stress ? stressAccount : 'PlayerOne', avatar: null, profileUrl: 'https://www.gog.com/u/' } : null,
    epic: signedStores.has('epic') ? { name: stress ? stressAccount : 'PlayerOne', avatar: null, profileUrl: 'https://www.epicgames.com/account/personal' } : null,
  }
  const deal = (id, title, shop, cut, price, regular, appId, url, lowest = null) => ({
    id, title, shop, url: url || `https://store.steampowered.com/app/${appId}`, image: null,
    cut, price, regular, lowest, appId,
  })
  const wishlistItems = [
    { store: 'steam', id: 'steam-440', appId: '440', name: 'Team Fortress 2', image: null, url: 'https://store.steampowered.com/app/440', priceInfo: { discount: 50, final: 4.99, initial: 9.99, formatted: '$4.99', currency: 'USD' }, cheapest: { store: 'GOG', price: 3.99, url: 'https://www.gog.com/game/classic_quest' }, lowest: 4.99 },
    { store: 'gog', id: 'gog-123456', appId: null, name: 'Classic Quest', image: null, url: 'https://www.gog.com/game/classic_quest', priceInfo: { discount: 0, final: 9.99, initial: 9.99, formatted: '$9.99', currency: 'USD' }, cheapest: null, lowest: 9.99 },
    { store: 'epic', id: 'epic-123', appId: null, name: 'Skyward Bound', image: null, url: 'https://store.epicgames.com/', priceInfo: { discount: 20, final: 7.99, initial: 9.99, formatted: '$7.99', currency: 'USD' }, cheapest: null, lowest: 5.99 },
  ]
  // ?bigwishlist=1: a huge public wishlist (paging and render-time checks); every 4th game on sale
  if (flags.get('bigwishlist') === '1') {
    for (let i = 1; i <= 3000; i++) {
      const store = ['steam', 'gog', 'epic'][i % 3], discount = i % 4 ? 0 : 20 + (i % 8) * 10, initial = 9.99 + (i % 50)
      const final = Math.round(initial * (100 - discount)) / 100
      wishlistItems.push({ store, id: `${store}-big-${i}`, appId: store === 'steam' ? String(100000 + i) : null, name: `Big Wishlist Game ${i}`, image: '/renderer/logo.png',
        url: store === 'steam' ? `https://store.steampowered.com/app/${100000 + i}` : store === 'gog' ? 'https://www.gog.com/game/classic_quest' : 'https://store.epicgames.com/',
        priceInfo: { discount, final, initial, formatted: `$${final.toFixed(2)}`, currency: 'USD' }, cheapest: null, lowest: i % 8 ? null : final })
    }
  }
  const sample = {
    freeGames: empty ? [] : [
      { id: 'epic-sample-free', title: 'The Long Way Home', source: 'epic', url: 'https://store.epicgames.com/', image: null, appId: null, endDate: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(), type: 'giveaway' },
      { id: 'steam-440', title: 'Team Fortress 2', source: 'steam', url: 'https://store.steampowered.com/app/440', image: null, appId: '440', endDate: new Date(Date.now() + 7 * 60 * 60 * 1000).toISOString(), type: 'free' },
      { id: 'steam-ftk-4841320', title: 'Squad Weapon Skins - Recon Team Pack (DLC)', source: 'steam', url: 'https://store.steampowered.com/app/4841320', image: null, appId: '4841320', endDate: null, type: 'free' },
      { id: 'gog-123456', title: 'Classic Quest', source: 'gog', url: 'https://www.gog.com/game/classic_quest', image: null, appId: null, endDate: null, type: 'giveaway' },
      { id: 'other-demo', title: 'Community Game', source: 'other', url: 'https://example.invalid/game', image: null, appId: null, endDate: null, type: 'giveaway' },
    ],
    freeWeekends: empty ? [] : [
      { id: 'steam-fw-570', title: 'Dota 2 Weekend Trial', source: 'steam', url: 'https://store.steampowered.com/app/570', image: null, appId: '570', endDate: new Date(Date.now() + 2 * 24 * 60 * 60 * 1000).toISOString(), type: 'weekend' },
    ],
    freeUpcoming: empty ? [] : [
      { id: 'epic-next-sample-1', title: 'Lanterns of the Deep', source: 'epic', url: 'https://store.epicgames.com/free-games', image: null, appId: null, endDate: null, starts: new Date(Date.now() + 4 * 24 * 60 * 60 * 1000).toISOString(), type: 'upcoming' },
      { id: 'epic-next-sample-2', title: 'Clockwork Orchard', source: 'epic', url: 'https://store.epicgames.com/free-games', image: null, appId: null, endDate: null, starts: new Date(Date.now() + 4 * 24 * 60 * 60 * 1000).toISOString(), type: 'upcoming' },
    ],
    // Like main.rs: only fetched (and sent) while Settings → Show free DLC & loot is on
    freeLoot: empty ? [] : [
      { id: 'gp-loot-3818', title: 'Squad Weapon Skins - Recon Team Pack', source: 'steam', url: 'https://www.gamerpower.com/open/squad-weapon-skins-recon-team-pack-steam-giveaway', image: null, appId: null, endDate: new Date(Date.now() + 6 * 24 * 60 * 60 * 1000).toISOString(), type: 'loot' },
      { id: 'gp-loot-3815', title: 'World of Tanks: Halloween Gift Packs', source: 'other', url: 'https://www.gamerpower.com/open/world-of-tanks-halloween-gift-packs', image: null, appId: null, endDate: null, type: 'loot' },
    ],
    sales: empty ? { steam: [], gog: [], epic: [] } : {
      steam: [
        deal('steam-sale-440', 'Team Fortress 2', 'Steam', 20, 7.99, 9.99, '440'),
        deal('steam-sale-441', 'Garden Tactics', 'Steam', 55, 13.49, 29.99, '441'),
        deal('steam-sale-442', 'Skyward Bound', 'Steam', 55, 8.99, 19.99, '442'),
        deal('steam-sale-443', 'The Long Way Home', 'Steam', 80, 5.99, 29.99, '443'),
        deal('steam-sale-444', 'Starlight Courier', 'Steam', 80, 3.99, 19.99, '444'),
        deal('steam-sale-445', 'Hollow Harbor', 'Steam', 95, 1.49, 29.99, '445'),
      ],
      gog: [
        deal('gog-sale-123456', 'Classic Quest', 'GOG', 20, 7.99, 9.99, '123456', 'https://www.gog.com/game/classic_quest'),
        deal('gog-sale-123457', 'The Long Way Home', 'GOG', 55, 13.49, 29.99, '123457', 'https://www.gog.com/game/the_long_way_home'),
        deal('gog-sale-123458', 'Skyward Bound', 'GOG', 55, 8.99, 19.99, '123458', 'https://www.gog.com/game/skyward_bound'),
        deal('gog-sale-123459', 'Garden Tactics', 'GOG', 80, 5.99, 29.99, '123459', 'https://www.gog.com/game/garden_tactics'),
        deal('gog-sale-123460', 'Starlight Courier', 'GOG', 80, 3.99, 19.99, '123460', 'https://www.gog.com/game/starlight_courier'),
        deal('gog-sale-123461', 'Hollow Harbor', 'GOG', 95, 1.49, 29.99, '123461', 'https://www.gog.com/game/hollow_harbor'),
      ],
      epic: [
        deal('epic-sale-123', 'Skyward Bound', 'Epic Games', 20, 15.99, 19.99, '123', 'https://store.epicgames.com/p/skyward-bound'),
        deal('epic-sale-124', 'Garden Tactics', 'Epic Games', 55, 13.49, 29.99, '124', 'https://store.epicgames.com/p/garden-tactics'),
        deal('epic-sale-125', 'The Long Way Home', 'Epic Games', 55, 8.99, 19.99, '125', 'https://store.epicgames.com/p/the-long-way-home'),
        deal('epic-sale-126', 'Starlight Courier', 'Epic Games', 80, 5.99, 29.99, '126', 'https://store.epicgames.com/p/starlight-courier'),
        deal('epic-sale-127', 'Hollow Harbor', 'Epic Games', 80, 3.99, 19.99, '127', 'https://store.epicgames.com/p/hollow-harbor'),
        deal('epic-sale-128', 'Classic Quest', 'Epic Games', 95, 1.49, 29.99, '128', 'https://store.epicgames.com/p/classic-quest'),
      ],
    },
    // ?sale=<key> (lunar, spring, summer, halloween, autumn, winter, or anything else for a sale with no theme)
    liveSale: flags.get('sale') ? { key: flags.get('sale'), name: `Steam ${flags.get('sale')} sale`, url: 'https://store.steampowered.com/specials' } : null,
    deals: empty ? [] : [
      { ...deal('deal-440', 'Team Fortress 2', 'Steam', 20, 7.99, 9.99, '440', undefined, 7.99), rating: 97, ratingText: 'Overwhelmingly Positive' },
      { ...deal('deal-123', 'Skyward Bound', 'GOG', 55, 13.49, 29.99, '123', 'https://www.gog.com/game/skyward_bound', 13.49), rating: 88, ratingText: 'Very Positive' },
      deal('deal-456', 'Garden Tactics', 'Epic Games', 80, 5.99, 29.99, '456', 'https://store.epicgames.com/p/garden-tactics'),
      { ...deal('deal-789', 'The Long Way Home', 'GOG', 95, 1.49, 29.99, '789', 'https://www.gog.com/game/the_long_way_home'), rating: 74, ratingText: 'Mostly Positive' },
      deal('deal-124', 'Starlight Courier', 'Steam', 50, 9.99, 19.99, '124'),
      deal('deal-125', 'Hollow Harbor', 'Epic Games', 75, 7.49, 29.99, '125', 'https://store.epicgames.com/p/hollow-harbor'),
    ],
    dealsSource: 'CheapShark', dealsError: null,
    wishlist: empty ? [] : wishlistItems.filter(item => signedStores.has(item.store)),
    wishlistPrivate: false,
    wishlistErrors: signedStores.has('gog') && flags.get('errors') === '1' ? { gog: 'Could not refresh wishlist' } : {},
    sourceErrors: flags.get('errors') === '1' ? ['Epic sales: HTTP 500', 'GOG giveaways: unavailable'] : [],
    lastChecked,
  }
  const syncWishlist = () => { sample.wishlist = empty ? [] : wishlistItems.filter(item => signedStores.has(item.store)) }
  if (stress) {
    sample.freeGames.unshift({ id: 'stress-free', title: stressTitle, source: 'steam', url: 'https://store.steampowered.com/app/999001', image: null, appId: '999001', endDate: null, type: 'free' })
    sample.freeLoot.unshift({ id: 'gp-loot-stress', title: stressTitle, source: 'other', url: 'https://www.gamerpower.com/open/stress', image: null, appId: null, endDate: null, type: 'loot' })
    sample.freeWeekends.unshift({ id: 'stress-weekend', title: stressTitle, source: 'steam', url: 'https://store.steampowered.com/app/999002', image: null, appId: '999002', endDate: new Date(Date.now() + 3 * 60 * 60 * 1000).toISOString(), type: 'weekend' })
    Object.entries(sample.sales).forEach(([store, items]) => {
      const shop = store === 'epic' ? 'Epic Games' : store === 'gog' ? 'GOG' : 'Steam'
      items.push(deal(`stress-${store}`, stressTitle, shop, 80, 1234.56, 6172.80, '999003'))
    })
    sample.deals.unshift({ ...deal('stress-deal', stressTitle, 'Steam', 80, 1234.56, 6172.80, '999004'), rating: 95, ratingText: 'Overwhelmingly Positive' })
    sample.wishlist.push({ store: 'steam', id: 'stress-wishlist', appId: '999005', name: stressTitle, image: null, url: 'https://store.steampowered.com/app/999005', priceInfo: { discount: 80, final: 1234.56, initial: 6172.80, formatted: '$1234.56' }, cheapest: null })
  }
  const emitters = new Set()
  const launcherOptions = {
    steam: [{ key: 'native', label: 'Steam' }],
    gog: [{ key: 'native', label: 'GOG Galaxy' }],
    epic: [{ key: 'native', label: 'Epic Games' }, { key: 'heroic', label: 'Heroic' }],
  }
  const launchers = () => Object.fromEntries(Object.entries(launcherOptions).map(([store, options]) => {
    const pref = store === 'gog' ? settings.launcherGog : store === 'epic' ? settings.launcherEpic : 'auto'
    const chosen = pref === 'browser' ? null : options.find(option => option.key === pref) || options[0] || null
    return [store, { options: clone(options), chosen: clone(chosen) }]
  }))
  const clone = value => JSON.parse(JSON.stringify(value))
  const snapshot = () => clone({ ...sample, freeLoot: settings.showLoot ? sample.freeLoot : [] })
  async function invoke (cmd, args = {}) {
    switch (cmd) {
      case 'get_settings': return clone(settings)
      case 'save_settings': {
        const lootOn = !settings.showLoot && args.settings?.showLoot
        settings = { ...settings, ...(args.settings || {}) }
        // main.rs wakes the poll when loot is switched on
        if (lootOn) setTimeout(() => { const payload = snapshot(); emitters.forEach(callback => callback({ payload })) }, 300)
        return undefined
      }
      case 'fetch_data':
      case 'check_now': sample.lastChecked = Date.now(); return snapshot()
      case 'get_accounts': return clone(accounts)
      case 'launchers': return launchers()
      // ?update=1 offers v99.0.0 (installing it fails, to show the error path); ?update=fail makes the check fail
      case 'check_update':
        if (flags.get('update') === 'fail') throw new Error('Preview: GitHub unreachable')
        return flags.get('update') ? { version: '99.0.0', page: 'https://github.com/SusieeTheLinuxUser/dealdrop/releases/tag/v99.0.0', canInstall: true } : null
      case 'install_update': await new Promise(resolve => setTimeout(resolve, 800)); throw new Error('Preview: Flatpak couldn\'t install the update')
      case 'get_autostart': return autostart
      case 'set_autostart': autostart = args.on; return undefined
      case 'steam_login': accounts.steam = { name: 'Preview Steam', avatar: null, profileUrl: 'https://steamcommunity.com/' }; signedStores.add('steam'); syncWishlist(); return clone(accounts)
      case 'gog_login': accounts.gog = { name: 'Preview GOG', avatar: null, profileUrl: 'https://www.gog.com/u/' }; signedStores.add('gog'); syncWishlist(); return clone(accounts)
      case 'epic_login': accounts.epic = { name: 'Preview Epic', avatar: null, profileUrl: 'https://www.epicgames.com/account/personal' }; signedStores.add('epic'); syncWishlist(); return clone(accounts)
      case 'logout': accounts[args.store] = null; signedStores.delete(args.store); syncWishlist(); return clone(accounts)
      case 'wishlist_game_deals': return { appId: args.appId, gameName: args.name, steam: { onSale: true, discount: 50, final: 4.99, initial: 9.99, formatted: '$4.99' }, cheapest: { store: 'GOG', price: 3.99, url: 'https://www.gog.com/game/classic_quest', savingsPercent: 60, retailPrice: 9.99 }, checkedAt: Date.now() }
      case 'open_url':
      case 'open_in_app': console.info(`[preview] ${cmd}`, args); return true
      case 'open_deal_in_app': console.info(`[preview] ${cmd}`, args); return true
      case 'window_minimize':
      case 'window_close': console.info(`[preview] ${cmd}`); return undefined
      default: throw new Error(`Preview bridge does not implement: ${cmd}`)
    }
  }
  window.__TAURI__ = {
    core: { invoke },
    // Only data-update is simulated; other events (the tray's navigate) never fire here.
    event: { listen: async (event, callback) => { if (event !== 'data-update') return () => {}; emitters.add(callback); return () => emitters.delete(callback) } },
    window: { getCurrentWindow: () => ({ startResizeDragging: direction => console.info('[preview] resize', direction) }) },
    app: { getVersion: async () => (await fetch('/src-tauri/tauri.conf.json').then(r => r.json()).catch(() => ({}))).version ?? '0.0.0' },
  }
  window.dealDropPreview = { flags, emit: payload => emitters.forEach(callback => callback({ payload })) }
})()
