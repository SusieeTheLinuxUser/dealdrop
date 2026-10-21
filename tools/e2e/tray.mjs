// Tray and notifications against fake desktop services (fake-desktop.py) on the private bus: the tray
// registers, its icon file is readable from outside the app (the Flatpak's sandbox too), the menu has its
// entries and counts, "Wishlist" opens that page, and a new free game sends one notification.
// Works with the release binary or the Flatpak (DEALDROP_BIN=tools/e2e/flatpak.sh).
import { launch, check, done, appArgs, sleep } from './wd.mjs'
import { spawn, execFileSync } from 'node:child_process'
import { createInterface } from 'node:readline'
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'

const events = []
const desktop = spawn('python3', ['-I', new URL('./fake-desktop.py', import.meta.url).pathname], { stdio: ['ignore', 'pipe', 'inherit'] })
createInterface({ input: desktop.stdout }).on('line', line => events.push(JSON.parse(line)))
const waitEvent = async (key, ms = 60000) => {
  for (const end = Date.now() + ms; Date.now() < end; await sleep(250)) { const e = events.find(e => e[key]); if (e) return e[key] }
  return null
}
await waitEvent('ready', 10000)

// Already primed with one id the store lists don't have: every real free game is "new" on the first poll
const home = process.env.HOME
mkdirSync(`${home}/.config/dealdrop`, { recursive: true })
writeFileSync(`${home}/.config/dealdrop/config.json`, JSON.stringify({ primed: true, seenFreeIds: ['e2e-old'], settings: { onboarded: true, notifyFreeGame: true } }))

const gdbus = (dest, path, method, ...args) =>
  execFileSync('gdbus', ['call', '--session', '--dest', dest, '--object-path', path, '--method', method, '--', ...args], { encoding: 'utf8' })

const app = await launch(appArgs)
try {
  const tray = await waitEvent('tray')
  check('tray icon registers with the StatusNotifierWatcher', !!tray, tray)
  if (tray) {
    const [dest, path] = tray.service.startsWith('/') ? [tray.sender, tray.service] : [tray.service, '/StatusNotifierItem']
    const props = gdbus(dest, path, 'org.freedesktop.DBus.Properties.GetAll', 'org.kde.StatusNotifierItem')
    const prop = name => props.match(new RegExp(`'${name}': <(?:objectpath )?'([^']*)'>`))?.[1]
    const [dir, icon] = [prop('IconThemePath'), prop('IconName')]
    // The panel runs outside the app and must find the icon by that path. A Flatpak sees its runtime folder as
    // /run/user/<uid>, which is the host's $XDG_RUNTIME_DIR (run.sh's temp one here, the same path on a real desktop).
    const outside = p => p?.replace(/^\/run\/user\/\d+(?=\/)/, process.env.XDG_RUNTIME_DIR)
    const file = icon?.startsWith('/') ? outside(icon) : outside(`${dir}/${icon}.png`)
    check('tray icon file is readable from outside the app', !!file && existsSync(file), { dir, icon, file })
    const menu = prop('Menu')
    const labels = async () => [...gdbus(dest, menu, 'com.canonical.dbusmenu.GetLayout', '0', '-1', '[]')
      .matchAll(/\((\d+), \{[^}]*?'label': <'([^']*)'>/g)].map(m => ({ id: Number(m[1]), label: m[2] }))
    await app.waitFor('return state.dataLoaded', 60000)
    await sleep(1000)
    const items = await labels()
    const names = items.map(i => i.label.replace(/ \(\d+\)$/, ''))
    check('tray menu entries', ['Open DealDrop', 'Free now', 'Wishlist', 'Refresh all', 'Quit'].every(n => names.includes(n)), items.map(i => i.label))
    const free = items.find(i => i.label.startsWith('Free now'))
    const count = Number(free?.label.match(/\((\d+)\)/)?.[1] ?? 0)
    const listed = await app.exec('return state.freeGames.length + (state.freeWeekends?.length ?? 0)')
    check('"Free now (N)" counts the free games and weekends', count === listed, { label: free?.label, listed })
    const wish = items.find(i => i.label.startsWith('Wishlist'))
    gdbus(dest, menu, 'com.canonical.dbusmenu.Event', String(wish.id), 'clicked', '<"">', '0')
    const page = await app.waitFor('return state.page === "wishlist" && state.page', 5000).catch(() => app.exec('return state.page'))
    check('tray "Wishlist" opens the Wishlist page', page === 'wishlist', page)
  }
  const note = await waitEvent('notify')
  check('a new free game sends a notification', !!note && /free/i.test(note.summary), note)
  check('…only one for the poll', events.filter(e => e.notify).length === 1, events.filter(e => e.notify).map(e => e.notify.summary))
} finally {
  await app.quit()
  desktop.kill()
}
done()
