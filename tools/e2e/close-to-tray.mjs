// Closing the window to the tray (the titlebar X → window_close → hide) fires `visibilitychange` in WebKitGTK,
// so the open page's "New" tags are marked seen (app.js visibilitychange listener).
import { launch, check, done, appArgs } from './wd.mjs'

const rafWithin1s = `const cb = arguments[arguments.length - 1]
  requestAnimationFrame(() => cb(true)); setTimeout(() => cb(false), 1000)`

const app = await launch(appArgs.filter(a => a !== '--hidden'))
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded && state.deals.length > 0', 60000)
  await app.exec('navigate("deals")')
  await app.execAsync(rafWithin1s)
  check('deals page is on screen', await app.exec('return state.pageShown && !document.hidden'), null)
  await app.exec('state.seen.deals = ["e2e-sentinel"]; window.__events = []; document.addEventListener("visibilitychange", () => window.__events.push(document.hidden))')
  await app.exec('return window.api.close()')
  const hidden = await app.waitFor('return document.hidden && "hidden"', 5000).catch(() => false)
  check('closing to the tray makes document.hidden true', hidden === 'hidden', await app.exec('return { hidden: document.hidden, events: window.__events }'))
  const seen = await app.exec('return { seen: state.seen.deals.length, deals: state.deals.length, pageShown: state.pageShown }')
  check('…and marks the open page seen', seen.seen > 1 && !seen.pageShown, seen)
} finally {
  await app.quit()
}
done()
