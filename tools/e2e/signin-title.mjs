// Sign-in window title shows the current site (f238dfa). Opens each store's sign-in, reads the native
// window title from the private KWin, closes the window. Never types anything.
import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { launch, check, done, sleep, appArgs } from './wd.mjs'

// KWin's window-search runner lists window captions (WebDriver's title is document.title, not the window's)
const kwinTitles = query => [...execFileSync('qdbus6', ['--literal', 'org.kde.KWin', '/WindowsRunner', 'org.kde.krunner1.Match', query])
  .toString().matchAll(/\(sssida\{sv\}\) "[^"]*", "([^"]*)"/g)].map(m => m[1])
// Close like the titlebar X would (a KWin script): WebDriver's close only drops the page, the window stays
function kwinClose (prefix) {
  const file = `${process.env.E2E_TMP}/close.js`
  writeFileSync(file, `workspace.windowList().filter(w => w.caption.startsWith(${JSON.stringify(prefix)})).forEach(w => w.closeWindow())`)
  const qdbus = (...a) => execFileSync('qdbus6', ['org.kde.KWin', ...a]).toString().trim()
  const id = qdbus('/Scripting', 'org.kde.kwin.Scripting.loadScript', file, `e2e-close-${Date.now()}`)
  qdbus(`/Scripting/Script${id}`, 'org.kde.kwin.Script.run')
}

const app = await launch(appArgs)
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded')
  const main = await app.handle()
  for (const [store, base, host] of [['steam', 'Sign in through Steam', 'steamcommunity.com'], ['gog', 'Sign in to GOG', 'login.gog.com'], ['epic', 'Sign in to Epic Games', 'www.epicgames.com']]) {
    await app.exec(`window.__e2e = 'pending'; window.api.login('${store}').then(() => { window.__e2e = 'resolved' }, e => { window.__e2e = 'rejected: ' + e })`)
    let titles = []
    for (let i = 0; i < 60 && !titles.some(t => t.includes(' · ')); i++) { await sleep(500); titles = kwinTitles(base) }
    check(`${store} window title`, titles.includes(`${base} · ${host}`), titles)

    const popup = (await app.handles()).find(h => h !== main)
    await app.switchTo(popup)
    console.log(`  ${store} page: ${await app.title()} — ${(await app.url()).split('?')[0]}`)
    await app.switchTo(main)
    kwinClose(base)
    const result = await app.waitFor('return window.__e2e !== "pending" && window.__e2e', 10000)
    check(`${store} closing the window cancels the sign-in`, result === 'rejected: Cancelled' && !kwinTitles(base).length, result)
  }
} finally {
  await app.quit()
}
done()
