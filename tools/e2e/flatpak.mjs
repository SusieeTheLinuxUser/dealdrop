// The installed Flatpak (DEALDROP_BIN=tools/e2e/flatpak.sh): the UI loads in the sandbox, config.json and the
// autostart entry are the host's files (update::use_host_dirs), an AppImage-era autostart entry becomes
// `flatpak run`, and the update check answers without an error.
import { launch, check, done, appArgs, sleep } from './wd.mjs'
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs'

const home = process.env.HOME // run.sh's throwaway home: what the sandbox sees as the host's
const autostart = `${home}/.config/autostart/dealdrop.desktop`
mkdirSync(`${home}/.config/autostart`, { recursive: true })
writeFileSync(autostart, '[Desktop Entry]\nType=Application\nName=DealDrop\nExec="/gone/DealDrop_0.9.2_amd64.AppImage" --hidden\nIcon=/i.png\nHidden=true\n')

const app = await launch(appArgs)
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded', 60000)
  check('UI loaded in the sandbox', true, await app.exec('return document.title'))
  const config = `${home}/.config/dealdrop/config.json`
  await app.exec('return window.api.saveSettings(state.settings)')
  check('config.json is the host file', existsSync(config), config)
  // Flatpak leaves an empty mount point there for the granted folder; a config.json in it would be a second copy
  const sandboxCopy = `${home}/.var/app/com.dealdrop.desktop/config/dealdrop/config.json`
  check('no second config.json under ~/.var/app', !existsSync(sandboxCopy), sandboxCopy)
  const entry = readFileSync(autostart, 'utf8')
  check('AppImage autostart entry now runs the Flatpak', entry.includes('\nExec=flatpak run com.dealdrop.desktop --hidden\n'), entry)
  check('…and keeps the desktop\'s own settings', entry.includes('Hidden=true') && entry.includes('Icon=/i.png'), entry)
  const up = await app.execAsync('const cb = arguments[arguments.length - 1]; window.api.checkUpdate().then(u => cb({ ok: true, u }), e => cb({ ok: false, e: String(e) }))')
  check('update check answers', up.ok, up)
  check('update button matches it', await app.exec('return document.getElementById("sidebar-update").style.display') === (up.u ? 'block' : 'none'), up.u)
  // A newer release that can't be installed here: the button names it (clicking would open the release page)
  const btn = await app.execAsync(`const cb = arguments[arguments.length - 1]
    window.api.checkUpdate = async () => ({ version: '99.0.0', page: 'https://github.com/SusieeTheLinuxUser/dealdrop/releases/tag/v99.0.0', canInstall: false })
    checkUpdate().then(() => { const b = document.getElementById('btn-update'); cb({ shown: b.offsetParent !== null, text: b.textContent }) })`)
  check('update button shows a newer release', btn.shown && btn.text === 'Update to v99.0.0', btn)
  if (process.env.E2E_SCREENSHOT) writeFileSync(process.env.E2E_SCREENSHOT, Buffer.from(await app.screenshot(), 'base64'))
  // WebKit storage ("New" tags, claimed games) is the host's ~/.local/share/com.dealdrop.desktop, like the AppImage's
  await app.exec('localStorage.setItem("e2e-probe", "1")')
  const storage = `${home}/.local/share/com.dealdrop.desktop/localstorage`
  for (let i = 0; i < 40 && !existsSync(storage); i++) await sleep(250)
  check('webview storage is the host folder', existsSync(storage), storage)
} finally {
  await app.quit()
}
done()
