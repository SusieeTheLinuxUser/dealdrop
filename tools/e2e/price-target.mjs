// A wishlist price target in the real app: a target above a game's current price sends one notification,
// the next poll doesn't repeat it, and a target that changes fires again. Needs a public Steam wishlist:
// STEAM_ID=7656… tools/e2e/run.sh tools/e2e/price-target.mjs
import { launch, check, done, appArgs, sleep } from './wd.mjs'
import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'
import { mkdirSync, writeFileSync } from 'node:fs'

const steamId = process.env.STEAM_ID
if (!/^\d{17}$/.test(steamId ?? '')) { console.log('set STEAM_ID to a 17-digit SteamID64 with a public wishlist'); process.exit(2) }
const events = []
const desktop = spawn('python3', ['-I', new URL('./fake-desktop.py', import.meta.url).pathname], { stdio: ['ignore', 'pipe', 'inherit'] })
createInterface({ input: desktop.stdout }).on('line', line => events.push(JSON.parse(line)))
const notes = () => events.filter(e => e.notify).map(e => e.notify)
for (let i = 0; i < 40 && !events.length; i++) await sleep(250)
// Only price targets may notify here
const quiet = { onboarded: true, notifyMajorSale: false, notifySaleSoon: false, notifyFreeGame: false, notifyFreeWeekend: false, notifyWishlistSale: false, notifyBigDeal: false }
mkdirSync(`${process.env.HOME}/.config/dealdrop`, { recursive: true })
writeFileSync(`${process.env.HOME}/.config/dealdrop/config.json`, JSON.stringify({ steamId, settings: quiet }))

const app = await launch(appArgs)
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded && state.wishlist.length > 0', 90000)
  const pick = await app.exec(`const w = state.wishlist.find(w => w.store === 'steam' && w.priceInfo?.currency === 'USD' && w.priceInfo.final > 1)
    return w && { key: 'steam:' + w.id, name: w.name, price: Math.min(w.priceInfo.final, w.cheapest?.price ?? Infinity) }`)
  check('a priced Steam wishlist game', !!pick, pick)
  const before = notes().length
  const setTarget = t => app.execAsync(`const cb = arguments[arguments.length - 1]
    state.settings.priceTargets = { [${JSON.stringify(pick.key)}]: ${t} }
    window.api.saveSettings(state.settings).then(() => window.api.checkNow()).then(() => cb(true), e => cb(String(e)))`)
  await setTarget(Math.ceil(pick.price + 1))
  await sleep(1000)
  const hit = notes().slice(before)
  check('target above the price: one notification naming the game', hit.length === 1 && (hit[0].summary + hit[0].body).includes(pick.name), hit)
  await app.execAsync('const cb = arguments[arguments.length - 1]; window.api.checkNow().then(() => cb(true))')
  await sleep(1000)
  check('the next poll doesn\'t repeat it', notes().length === before + 1, notes().slice(before).map(n => n.summary))
  await setTarget(Math.ceil(pick.price + 2))
  await sleep(1000)
  check('a changed target fires again', notes().length === before + 2, notes().slice(before).map(n => n.summary))
} finally {
  await app.quit()
  desktop.kill()
}
done()
