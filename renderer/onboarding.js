(() => {
  let step = 0, dialog = null, returnFocus = null
  const api = () => window.dealDropOnboarding
  const labels = ['Welcome', 'Look', 'Notifications', 'Stores & launchers', 'Done']
  const button = (text, cls, fn) => { const b = document.createElement('button'); b.type = 'button'; b.className = cls; b.textContent = text; b.addEventListener('click', fn); return b }
  function start () {
    if (dialog) return
    returnFocus = document.activeElement; step = 0
    dialog = document.createElement('div'); dialog.className = 'onboarding-backdrop'
    dialog.innerHTML = '<section class="onboarding-dialog" role="dialog" aria-modal="true" aria-labelledby="onboarding-title" aria-live="polite"><div class="onboarding-head"><div class="onboarding-dots"></div><span class="onboarding-count"></span></div><div class="onboarding-content"></div><div class="onboarding-error" role="status"></div><div class="onboarding-actions"></div></section>'
    document.body.appendChild(dialog); document.addEventListener('keydown', keydown); render()
  }
  function finish () {
    if (!dialog) return
    api().state.settings.onboarded = true
    window.api.saveSettings(api().state.settings)
    document.removeEventListener('keydown', keydown); dialog.remove(); dialog = null
    returnFocus?.focus?.()
  }
  function keydown (e) {
    if (e.key === 'Escape') { e.preventDefault(); finish(); return }
    if (e.key !== 'Tab' || !dialog) return
    const items = [...dialog.querySelectorAll('button:not(:disabled),select,a[href]')]
    if (!items.length) { e.preventDefault(); return }
    if (e.shiftKey && document.activeElement === items[0]) { e.preventDefault(); items.at(-1).focus() }
    else if (!e.shiftKey && document.activeElement === items.at(-1)) { e.preventDefault(); items[0].focus() }
  }
  function render () {
    const {state, baseThemes, lookToggles, notificationToggles, applyTheme, setAccounts} = api()
    const settings = state.settings, body = dialog.querySelector('.onboarding-content')
    dialog.querySelector('.onboarding-count').textContent = 'Step ' + (step + 1) + ' of 5'
    const dots = dialog.querySelector('.onboarding-dots'); dots.replaceChildren()
    labels.forEach((_, i) => { const dot = document.createElement('i'); dot.className = 'onboarding-dot' + (i === step ? ' active' : ''); dots.appendChild(dot) })
    body.replaceChildren()
    const title = document.createElement('h2'); title.id = 'onboarding-title'; title.textContent = labels[step]; body.appendChild(title)
    const p = text => { const el = document.createElement('p'); el.className = 'onboarding-copy'; el.textContent = text; body.appendChild(el) }
    const toggle = row => {
      const wrap = document.createElement('div'); wrap.className = 'onboarding-toggle-row'
      const copy = document.createElement('div'), label = document.createElement('strong'), sub = document.createElement('small')
      label.textContent = row.label; sub.textContent = row.sub; copy.append(label, sub)
      if (row.gamerpower) { sub.replaceChildren(document.createTextNode('Epic, Steam, GOG & ')); const a = document.createElement('a'); a.href = '#'; a.dataset.href = 'https://www.gamerpower.com'; a.textContent = 'GamerPower'; sub.append(a, document.createTextNode(' giveaways')) }
      const b = button('', 'onboarding-toggle' + (settings[row.key] ? ' on' : ''), () => { settings[row.key] = !settings[row.key]; b.classList.toggle('on', settings[row.key]); b.setAttribute('aria-pressed', String(settings[row.key])); if (row.key === 'rounded' || row.key === 'effects') api().applyTheme() })
      b.setAttribute('aria-label', row.label); b.setAttribute('aria-pressed', String(Boolean(settings[row.key])))
      wrap.append(copy, b); return wrap
    }
    if (step === 0) p('Find free games, store sales, hot deals and wishlist price drops, with tray notifications. No accounts are required.')
    if (step === 1) {
      p('Pick a look. You can change it anytime in Settings.')
      const themes = document.createElement('div'); themes.className = 'onboarding-choices'
      Object.entries(baseThemes).forEach(([key, theme]) => { const b = button(theme.name, 'onboarding-choice' + (settings.theme === key ? ' active' : ''), () => { settings.theme = key; settings.baseTheme = key; applyTheme(); render() }); b.setAttribute('aria-pressed', String(settings.theme === key)); themes.appendChild(b) })
      body.appendChild(themes); lookToggles.forEach(row => body.appendChild(toggle(row)))
    }
    if (step === 2) {
      p('The app lives in your tray. Closing this window hides it; choose Quit from the tray menu to exit.')
      notificationToggles.forEach(row => body.appendChild(toggle(row)))
      const label = document.createElement('strong'); label.className = 'onboarding-label'; label.textContent = 'Check every'; body.appendChild(label)
      const choices = document.createElement('div'); choices.className = 'onboarding-choices'
      ;[15,60,240,720].forEach(n => { const name = n < 60 ? n + ' min' : n / 60 + ' hr'; const b = button(name, 'onboarding-choice' + (settings.checkInterval === n ? ' active' : ''), () => { settings.checkInterval = n; render() }); b.setAttribute('aria-pressed', String(settings.checkInterval === n)); choices.appendChild(b) })
      body.appendChild(choices)
    }
    if (step === 3) {
      p('Sign-in is optional and enables wishlist alerts. Tokens stay on this computer; you enter your password on the store’s own page.')
      ;[['steam','Steam'],['gog','GOG'],['epic','Epic Games']].forEach(([key,name]) => {
        const row = document.createElement('div'); row.className = 'onboarding-account'
        const account = state.accounts[key], status = document.createElement('span')
        status.textContent = account ? name + ': signed in as ' + (account.name || name + ' user') : name + ': not signed in'
        if (account?.avatar) { const image = document.createElement('img'); image.src = account.avatar; image.alt = ''; row.appendChild(image) }
        row.appendChild(status)
        if (!account) row.appendChild(button('Sign in', 'onboarding-secondary', async e => {
          const b = e.currentTarget; b.disabled = true; b.textContent = 'Opening…'
          try { setAccounts(await window.api.login(key)); await api().loadData(); render() }
          catch { dialog.querySelector('.onboarding-error').textContent = 'Sign-in did not finish. You can try later in Settings.'; b.disabled = false; b.textContent = 'Try again' }
        }))
        body.appendChild(row)
        if (key !== 'steam' && (state.launchers[key]?.options?.length ?? 0) > 1) {
          const pref = key === 'gog' ? 'launcherGog' : 'launcherEpic', label = document.createElement('label')
          label.textContent = 'Open ' + name + ' games in '
          const select = document.createElement('select'); select.setAttribute('aria-label', 'Open ' + name + ' games in')
          ;[{key:'auto',label:'Auto'}, ...state.launchers[key].options, {key:'browser',label:'Browser'}].forEach(o => { const option = document.createElement('option'); option.value = o.key; option.textContent = o.label; select.appendChild(option) })
          select.value = settings[pref] || 'auto'; select.addEventListener('change', () => { settings[pref] = select.value })
          label.appendChild(select); body.appendChild(label)
        }
      })
    }
    if (step === 4) {
      p('You’re all set. You can change these choices anytime in Settings.')
      const summary = document.createElement('p'); summary.className = 'onboarding-summary'
      summary.textContent = 'Theme: ' + (baseThemes[settings.baseTheme]?.name || 'Midnight') + ' · Checks: every ' + (settings.checkInterval < 60 ? settings.checkInterval + ' min' : settings.checkInterval / 60 + ' hr')
      body.appendChild(summary)
    }
    const actions = dialog.querySelector('.onboarding-actions'); actions.replaceChildren()
    if (step) actions.appendChild(button('Back', 'onboarding-secondary', () => { step--; render() }))
    if (step < 4) actions.appendChild(button('Skip', 'onboarding-skip', finish))
    actions.appendChild(button(step === 4 ? 'Finish' : 'Next', 'onboarding-primary', () => { if (step === 4) finish(); else { step++; render() } }))
    actions.querySelector('button:last-child').focus()
  }
  window.startOnboarding = start
})()
