// What the installed Flatpak offers for a newer release, with the real `flatpak info` answering (fake-flatpak-host.py).
// E2E_EXPECT=repo: no button, About says how to get it, installing is refused. E2E_EXPECT=bundle: the button is there.
// The build must be older than the latest GitHub release, or there's nothing to offer. Run through flatpak-repo.sh.
import { launch, check, done, appArgs } from './wd.mjs'
import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'

const repo = process.env.E2E_EXPECT === 'repo'
const host = spawn('python3', ['-I', new URL('./fake-flatpak-host.py', import.meta.url).pathname], { stdio: ['ignore', 'pipe', 'inherit'] })
await new Promise(resolve => createInterface({ input: host.stdout }).once('line', resolve))

const app = await launch(appArgs)
try {
  await app.waitFor('return typeof state === "object" && state.dataLoaded', 60000)
  const r = await app.execAsync(`const cb = arguments[arguments.length - 1]
    checkUpdateNow().then(() => { const b = document.getElementById('btn-update')
      cb({ up: state.update, shown: document.getElementById('sidebar-update').style.display, text: b.textContent, about: document.getElementById('about-update-status').textContent }) })`)
  check('a newer release is on offer', !!r.up, r)
  check('origin decides how it is offered', r.up?.viaRepo === repo && r.up?.canInstall === !repo, r)
  check('sidebar button', r.shown === (repo ? 'none' : 'block'), r)
  check('About says what to do', repo ? r.about.includes('flatpak update') : r.about.includes('in the sidebar'), r)
  if (repo) {
    const e = await app.execAsync('const cb = arguments[arguments.length - 1]; window.api.installUpdate().then(() => cb(null), e => cb(String(e)))')
    check('installing is refused', /flatpak update/.test(e ?? ''), e)
  } else check('button names the version', r.text === `Update to v${r.up?.version}`, r)
} finally {
  await app.quit()
  host.kill()
}
done()
