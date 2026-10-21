// `--hidden` start (4750aa9): a page that was never on screen isn't marked seen. The shown run proves the guard isn't always skipping.
import { launch, check, done, appArgs } from './wd.mjs'

const rafWithin1s = `const cb = arguments[arguments.length - 1]
  requestAnimationFrame(() => cb(true)); setTimeout(() => cb(false), 1000)`

for (const hidden of [true, false]) {
  const tag = hidden ? '[--hidden]' : '[shown]'
  const app = await launch(hidden ? [...appArgs, '--hidden'] : appArgs.filter(a => a !== '--hidden'))
  try {
    await app.waitFor('return typeof state === "object" && state.dataLoaded')
    const vis = await app.exec('return { hidden: document.hidden, visibilityState: document.visibilityState, page: state.page }')
    check(`${tag} document.hidden`, vis.hidden === hidden, vis)
    const raf = await app.execAsync(rafWithin1s)
    check(`${tag} requestAnimationFrame runs within 1s`, raf === !hidden, raf)

    // The tray's "Free now" path: leave a seen-tracked page for 'free'
    await app.exec('navigate("deals")')
    await app.execAsync(rafWithin1s)
    const shown = await app.exec('return state.pageShown')
    check(`${tag} state.pageShown on deals`, shown === !hidden, shown)
    const after = await app.exec('state.seen.deals = ["e2e-sentinel"]; navigate("free"); return state.seen.deals')
    const kept = JSON.stringify(after) === '["e2e-sentinel"]'
    check(`${tag} navigate('free') ${hidden ? 'keeps' : 'records'} state.seen.deals`, kept === hidden, after.length > 5 ? `${after.length} ids` : after)
  } finally {
    await app.quit()
  }
}
done()
