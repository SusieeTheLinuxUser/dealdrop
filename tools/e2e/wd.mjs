// Minimal W3C WebDriver client for WebKitWebDriver (started by run.sh). No dependencies.
const base = process.env.WEBDRIVER_URL
if (!base) throw new Error('run through tools/e2e/run.sh')

export const appArgs = (process.env.E2E_APP_ARGS || '').split('\n').filter(Boolean)
export const sleep = ms => new Promise(r => setTimeout(r, ms))

async function call (method, path, body) {
  const res = await fetch(base + path, { method, headers: { 'content-type': 'application/json' }, body: body && JSON.stringify(body) })
  const { value } = await res.json()
  if (value?.error) throw new Error(`${method} ${path}: ${value.error}: ${value.message}`)
  return value
}

// Launches the app; quit() ends the session, which stops it
export async function launch (args = appArgs) {
  const { sessionId } = await call('POST', '/session', {
    capabilities: { alwaysMatch: { 'webkitgtk:browserOptions': { binary: process.env.DEALDROP_BIN, args } } },
  })
  const s = (method, path, body) => call(method, `/session/${sessionId}${path}`, body)
  return {
    // `script` is a function body; app.js top-level bindings (`state`, `navigate`, …) are in scope
    exec: (script, ...args) => s('POST', '/execute/sync', { script, args }),
    // the callback is arguments[arguments.length - 1]
    execAsync: (script, ...args) => s('POST', '/execute/async', { script, args }),
    // retry `script` until it returns something truthy
    async waitFor (script, ms = 30000) {
      for (const end = Date.now() + ms; Date.now() < end; await sleep(250)) {
        const v = await s('POST', '/execute/sync', { script, args: [] })
        if (v) return v
      }
      throw new Error(`timed out waiting for: ${script}`)
    },
    handles: () => s('GET', '/window/handles'),
    handle: () => s('GET', '/window'),
    switchTo: handle => s('POST', '/window', { handle }),
    title: () => s('GET', '/title'),
    url: () => s('GET', '/url'),
    quit: () => s('DELETE', ''),
  }
}

let failed = 0
export function check (name, ok, observed) {
  if (!ok) failed++
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name} — ${JSON.stringify(observed)}`)
}
export const done = () => { console.log(failed ? `${failed} failed` : 'all passed'); process.exitCode = failed ? 1 : 0 }
