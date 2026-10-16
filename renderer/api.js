// Bridge between the UI and the Rust backend (src-tauri/src/main.rs).
// Same window.api surface the Electron preload used to expose.
const { invoke } = window.__TAURI__.core
const { listen } = window.__TAURI__.event

window.api = {
  getSettings:    ()    => invoke('get_settings'),
  saveSettings:   s     => invoke('save_settings', { settings: s }),
  hasKey:         name  => invoke('has_key', { name }),
  saveKey:        (name, value) => invoke('save_key', { name, value }),
  fetchData:      ()    => invoke('fetch_data'),
  checkNow:       ()    => invoke('check_now'),
  openUrl:        url   => invoke('open_url', { url }),
  openInApp:      (store, url, id) => invoke('open_in_app', { store, url, id: id ?? null }),
  launchers:      ()    => invoke('launchers'),
  minimize:       ()    => invoke('window_minimize'),
  close:          ()    => invoke('window_close'),
  getAccounts:    ()    => invoke('get_accounts'),
  login:          store => invoke(store + '_login'),
  logout:         store => invoke('logout', { store }),
  refreshProfile: ()    => invoke('refresh_profile'),
  fetchWishlistGameDeals: p => invoke('wishlist_game_deals', { appId: p.appId, name: p.name }),
  onDataUpdate:   cb    => listen('data-update', e => cb(e.payload)),
}
