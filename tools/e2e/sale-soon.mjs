// "Steam … Sale starts tomorrow": the release binary with its wall clock moved to noon UTC the day before the
// next sale in the calendar (clock-shift.c), so the first poll sends the heads-up once and a second poll doesn't.
// Release binary only (a Flatpak doesn't pass LD_PRELOAD through).
import { launch, check, done, appArgs, sleep } from './wd.mjs'
import { spawn, execFileSync } from 'node:child_process'
import { createInterface } from 'node:readline'
import { mkdirSync, writeFileSync } from 'node:fs'

const here = new URL('.', import.meta.url).pathname
const tmp = process.env.E2E_TMP
// Next start date in main.rs SALES (the app's own calendar), so this keeps working as dates pass
const sales = [...execFileSync('grep', ['-o', 'start: "[0-9-]*"', `${here}/../../src-tauri/src/main.rs`], { encoding: 'utf8' }).matchAll(/"([\d-]+)"/g)].map(m => m[1])
const start = sales.find(d => Date.parse(`${d}T00:00:00Z`) > Date.now() + 86400e3)
const dayBefore = Date.parse(`${start}T00:00:00Z`) - 12 * 3600e3
execFileSync('cc', ['-shared', '-fPIC', '-O2', '-o', `${tmp}/clock-shift.so`, `${here}/clock-shift.c`, '-ldl'])
writeFileSync(`${tmp}/shifted.sh`, `#!/bin/sh\nexport LD_PRELOAD=${tmp}/clock-shift.so CLOCK_SHIFT_SECONDS=${Math.round((dayBefore - Date.now()) / 1000)}\nexec "${process.env.DEALDROP_BIN}" "$@"\n`, { mode: 0o755 })
process.env.DEALDROP_BIN = `${tmp}/shifted.sh`

const events = []
const desktop = spawn('python3', ['-I', `${here}/fake-desktop.py`], { stdio: ['ignore', 'pipe', 'inherit'] })
createInterface({ input: desktop.stdout }).on('line', line => events.push(JSON.parse(line)))
const notes = () => events.filter(e => e.notify).map(e => e.notify.summary)
for (let i = 0; i < 40 && !events.length; i++) await sleep(250)
const quiet = { onboarded: true, notifyMajorSale: false, notifySaleSoon: true, notifyFreeGame: false, notifyFreeWeekend: false, notifyWishlistSale: false, notifyBigDeal: false }
mkdirSync(`${process.env.HOME}/.config/dealdrop`, { recursive: true })
writeFileSync(`${process.env.HOME}/.config/dealdrop/config.json`, JSON.stringify({ settings: quiet }))

const app = await launch(appArgs) // wd.mjs reads DEALDROP_BIN at launch
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded', 90000)
  const today = await app.exec('return new Date().toISOString().slice(0, 10)')
  check('the app\'s clock is the day before the sale', Date.parse(`${today}T00:00:00Z`) === Date.parse(`${start}T00:00:00Z`) - 86400e3, { today, start })
  await sleep(1000)
  check('one "starts tomorrow" notification', notes().length === 1 && /starts tomorrow/.test(notes()[0]), notes())
  await app.execAsync('const cb = arguments[arguments.length - 1]; window.api.checkNow().then(() => cb(true))')
  await sleep(1000)
  check('…not repeated by the next poll', notes().length === 1, notes())
} finally {
  await app.quit()
  desktop.kill()
}
done()
