// Bridge between the UI and the Rust backend (src-tauri/src/main.rs).
// Same window.api surface the Electron preload used to expose.
const { invoke } = window.__TAURI__.core
const { listen } = window.__TAURI__.event

window.api = {
  getSettings:    ()    => invoke('get_settings'),
  saveSettings:   s     => invoke('save_settings', { settings: s }),
  fetchData:      ()    => invoke('fetch_data'),
  checkNow:       ()    => invoke('check_now'),
  openUrl:        url   => invoke('open_url', { url }),
  openInApp:      (store, url, id) => invoke('open_in_app', { store, url, id: id ?? null }),
  openDealInApp:  (store, title, fallbackUrl) => invoke('open_deal_in_app', { store, title, fallbackUrl }),
  launchers:      ()    => invoke('launchers'),
  getAutostart:   ()    => invoke('get_autostart'),
  setAutostart:   on    => invoke('set_autostart', { on }),
  minimize:       ()    => invoke('window_minimize'),
  close:          ()    => invoke('window_close'),
  getAccounts:    ()    => invoke('get_accounts'),
  login:          store => invoke(store + '_login'),
  logout:         store => invoke('logout', { store }),
  checkUpdate:    ()    => invoke('check_update'),
  installUpdate:  ()    => invoke('install_update'),
  fetchWishlistGameDeals: p => invoke('wishlist_game_deals', { appId: p.appId, name: p.name }),
  onDataUpdate:   cb    => listen('data-update', e => cb(e.payload)),
  onNavigate:     cb    => listen('navigate', e => cb(e.payload)),
}
