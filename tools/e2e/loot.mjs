// Free DLC & loot (Settings → Free now, opt-in) in the real app: live GamerPower loot renders as LOOT cards,
// never repeats a free game (Steam's free DLC), never reaches the new-free-games notification or the tray count,
// and switching it off hides the section.
import { launch, check, done, appArgs, sleep } from './wd.mjs'
import { spawn, execFileSync } from 'node:child_process'
import { createInterface } from 'node:readline'
import { mkdirSync, writeFileSync } from 'node:fs'

const events = []
const desktop = spawn('python3', ['-I', new URL('./fake-desktop.py', import.meta.url).pathname], { stdio: ['ignore', 'pipe', 'inherit'] })
createInterface({ input: desktop.stdout }).on('line', line => events.push(JSON.parse(line)))
const waitEvent = async (key, ms = 60000) => {
  for (const end = Date.now() + ms; Date.now() < end; await sleep(250)) { const e = events.find(e => e[key]); if (e) return e[key] }
  return null
}
await waitEvent('ready', 10000)
const home = process.env.HOME
mkdirSync(`${home}/.config/dealdrop`, { recursive: true })
writeFileSync(`${home}/.config/dealdrop/config.json`, JSON.stringify({ primed: true, seenFreeIds: ['e2e-old'], settings: { onboarded: true, showLoot: true, notifyFreeGame: true } }))

const app = await launch(appArgs)
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded', 60000)
  await app.exec('navigate("free")')
  const r = await app.exec(`const grid = document.getElementById('grid-loot')
    return { loot: state.freeLoot.map(g => g.title), cards: grid.children.length, tags: [...grid.querySelectorAll('*')].filter(e => e.textContent.trim() === 'LOOT').length,
      label: getComputedStyle(document.getElementById('label-loot')).display, games: state.freeGames.length + (state.freeWeekends?.length ?? 0) }`)
  check('GamerPower loot arrived', r.loot.length > 0, r.loot.length)
  const key = t => t.replace(/ \(DLC\)$/, '').toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')
  const twice = await app.exec('return state.freeGames.map(g => g.title)').then(free => r.loot.filter(t => free.some(f => key(f) === key(t))))
  check('no loot card repeats a Free to keep game', twice.length === 0, twice)
  check('…rendered as LOOT cards under its label', r.cards === r.loot.length && r.tags >= r.cards && r.label !== 'none', { cards: r.cards, tags: r.tags, label: r.label })
  const note = await waitEvent('notify')
  const leaked = note ? r.loot.filter(t => note.body.includes(t)) : []
  check('the new-free-games notification leaves loot out', !!note && leaked.length === 0, { summary: note?.summary, leaked })
  const tray = await waitEvent('tray', 5000)
  if (tray) {
    const [dest, path] = tray.service.startsWith('/') ? [tray.sender, tray.service] : [tray.service, '/StatusNotifierItem']
    const gd = (p, m, ...a) => execFileSync('gdbus', ['call', '--session', '--dest', dest, '--object-path', p, '--method', m, '--', ...a], { encoding: 'utf8' })
    const menu = gd(path, 'org.freedesktop.DBus.Properties.Get', 'org.kde.StatusNotifierItem', 'Menu').match(/'([^']+)'/)[1]
    const free = gd(menu, 'com.canonical.dbusmenu.GetLayout', '0', '-1', '[]').match(/'label': <'(Free now[^']*)'>/)?.[1]
    check('tray "Free now" count leaves loot out', free === `Free now (${r.games})`, { free, games: r.games, loot: r.loot.length })
  }
  await app.exec('state.settings.showLoot = false; window.api.saveSettings(state.settings); navigate("free")')
  const off = await app.exec('return { label: getComputedStyle(document.getElementById("label-loot")).display, cards: document.getElementById("grid-loot").children.length }')
  check('switched off: the section is gone', off.label === 'none' && off.cards === 0, off)
} finally {
  await app.quit()
  desktop.kill()
}
done()
