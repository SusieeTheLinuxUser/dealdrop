const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('api', {
  getSettings:      ()      => ipcRenderer.invoke('get-settings'),
  saveSettings:     (s)     => ipcRenderer.invoke('save-settings', s),
  getItadKey:       ()      => ipcRenderer.invoke('get-itad-key'),
  saveItadKey:      (k)     => ipcRenderer.invoke('save-itad-key', k),
  getSteamApiKey:   ()      => ipcRenderer.invoke('get-steam-api-key'),
  saveSteamApiKey:  (k)     => ipcRenderer.invoke('save-steam-api-key', k),
  fetchData:        ()      => ipcRenderer.invoke('fetch-data'),
  checkNow:         ()      => ipcRenderer.invoke('check-now'),
  openUrl:          (url)   => ipcRenderer.invoke('open-url', url),
  openSteam:        (appId) => ipcRenderer.invoke('open-steam', appId),
  minimize:         ()      => ipcRenderer.invoke('window-minimize'),
  close:            ()      => ipcRenderer.invoke('window-close'),
  getSteamUser:     ()      => ipcRenderer.invoke('get-steam-user'),
  steamLogin:       ()      => ipcRenderer.invoke('steam-login'),
  steamLogout:      ()      => ipcRenderer.invoke('steam-logout'),
  refreshProfile:   ()      => ipcRenderer.invoke('refresh-profile'),
  onDataUpdate:     (cb)    => ipcRenderer.on('data-update', (_, d) => cb(d)),
})
