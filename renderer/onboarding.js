(() => {
  let step = 0, dialog = null, returnFocus = null, lastAnnouncedStep = -1
  let tourOverlay = null, tourTrigger = null, tourIndex = -1
  const api = () => window.dealDropOnboarding
  const labels = ['Welcome', 'Appearance', 'Notifications', 'Stores & launchers', 'Done']
  const tourStops = [
    { selector: '.nav-items', title: 'Find a page', copy: 'Use these buttons to switch between Wishlist, Free now, Store sales, Hot deals, Calendar and Settings.' },
    { selector: '#btn-check-now', title: 'Refresh all', copy: 'Refresh stores and wishlists whenever you want an update.' },
    { selector: '.nav-item[data-page="settings"]', title: 'Settings', copy: 'Change your appearance, notifications, check interval and launcher preferences here.' },
    { selector: null, title: 'Stay in the tray', copy: 'Closing this window hides DealDrop in the tray. Choose Quit from the tray menu to exit.' },
  ]
  const button = (text, cls, fn) => { const b = document.createElement('button'); b.type = 'button'; b.className = cls; b.textContent = text; b.addEventListener('click', fn); return b }
  function start () {
    if (dialog) return
    api().closeShortcutsDialog?.()
    api().state.onboardingOpen = true; api().state.onboardingThemePicked = false
    returnFocus = document.activeElement; step = 0; lastAnnouncedStep = -1
    dialog = document.createElement('div'); dialog.className = 'onboarding-backdrop'
    dialog.innerHTML = '<section class="onboarding-dialog" role="dialog" aria-modal="true" aria-labelledby="onboarding-title"><div class="onboarding-head"><div class="onboarding-dots" aria-hidden="true"></div><span class="onboarding-count"></span></div><span class="onboarding-sr-only" aria-live="polite" aria-atomic="true"></span><div class="onboarding-content"></div><div class="onboarding-error" role="status"></div><div class="onboarding-actions"></div></section>'
    document.body.appendChild(dialog); document.addEventListener('keydown', keydown); render()
  }
  function finish () {
    if (!dialog) return
    api().state.settings.onboarded = true
    api().state.onboardingOpen = false
    window.api.saveSettings(api().state.settings)
    document.removeEventListener('keydown', keydown); dialog.remove(); dialog = null
    if (api().state.page === 'settings') api().renderSettings()
    returnFocus?.focus?.()
  }
  function keydown (e) {
    if (e.key === 'Escape') { e.preventDefault(); if (tourOverlay) endTour(); else finish(); return }
    const layer = tourOverlay || dialog
    if (e.key !== 'Tab' || !layer) return
    const items = [...layer.querySelectorAll('button:not(:disabled),select,a[href]')]
    if (!items.length) { e.preventDefault(); return }
    if (!layer.contains(document.activeElement)) { e.preventDefault(); (e.shiftKey ? items.at(-1) : items[0]).focus() }
    else if (e.shiftKey && document.activeElement === items[0]) { e.preventDefault(); items.at(-1).focus() }
    else if (!e.shiftKey && document.activeElement === items.at(-1)) { e.preventDefault(); items[0].focus() }
  }
  function render () {
    const {state, baseThemes, lookToggles, notificationToggles, applyTheme, setAccounts} = api()
    const focusKey = dialog.contains(document.activeElement) ? document.activeElement.dataset.focusKey : null
    const stepChanged = lastAnnouncedStep !== step
    const settings = state.settings, body = dialog.querySelector('.onboarding-content')
    dialog.querySelector('.onboarding-error').textContent = ''
    dialog.querySelector('.onboarding-count').textContent = 'Step ' + (step + 1) + ' of 5'
    if (lastAnnouncedStep !== step) {
      dialog.querySelector('.onboarding-sr-only').textContent = 'Step ' + (step + 1) + ' of 5: ' + labels[step]
      lastAnnouncedStep = step
    }
    const dots = dialog.querySelector('.onboarding-dots'); dots.replaceChildren()
    labels.forEach((_, i) => { const dot = document.createElement('i'); dot.className = 'onboarding-dot' + (i === step ? ' active' : ''); dots.appendChild(dot) })
    body.replaceChildren()
    const title = document.createElement('h2'); title.id = 'onboarding-title'; title.textContent = labels[step]; body.appendChild(title)
    const p = text => { const el = document.createElement('p'); el.className = 'onboarding-copy'; el.textContent = text; body.appendChild(el) }
    const toggle = row => {
      const wrap = document.createElement('div'); wrap.className = 'onboarding-toggle-row'
      const copy = document.createElement('div'), label = document.createElement('strong'), sub = document.createElement('small')
      label.textContent = row.label; sub.className = 'onboarding-sub'; sub.textContent = row.sub; copy.append(label, sub)
      if (row.gamerpower) { sub.replaceChildren(document.createTextNode('Epic, Steam, GOG & ')); const a = document.createElement('a'); a.href = '#'; a.dataset.href = 'https://www.gamerpower.com'; a.textContent = 'GamerPower'; sub.append(a, document.createTextNode(' giveaways')) }
      const b = button('', 'onboarding-toggle' + (settings[row.key] ? ' on' : ''), () => { settings[row.key] = !settings[row.key]; b.classList.toggle('on', settings[row.key]); b.setAttribute('aria-pressed', String(settings[row.key])); if (row.key === 'rounded' || row.key === 'effects') api().applyTheme() })
      b.setAttribute('aria-label', row.label); b.setAttribute('aria-pressed', String(Boolean(settings[row.key])))
      wrap.append(copy, b); return wrap
    }
    if (step === 0) p('Find free games, store sales, hot deals and wishlist price drops, with tray notifications. No DealDrop account needed. Store sign-in is optional, for wishlists.')
    if (step === 1) {
      p('Pick a look. You can change it anytime in Settings.')
      const themes = document.createElement('div'); themes.className = 'onboarding-themes'
      Object.entries(baseThemes).forEach(([key, theme]) => {
        const selected = settings.theme === key || (!baseThemes[settings.theme] && settings.baseTheme === key)
        const b = button('', 'onboarding-choice' + (selected ? ' active' : ''), () => { api().state.onboardingThemePicked = true; settings.theme = key; settings.baseTheme = key; applyTheme(); render() })
        b.dataset.focusKey = 'theme-' + key; b.setAttribute('aria-pressed', String(selected))
        const swatch = document.createElement('span'); swatch.className = 'onboarding-swatch'; swatch.setAttribute('aria-hidden', 'true')
        theme.sw.forEach((_, i) => { const color = document.createElement('i'); color.className = 'onboarding-swatch-color ' + key + '-' + i; swatch.appendChild(color) })
        b.append(swatch, document.createTextNode(theme.name)); themes.appendChild(b)
      })
      body.appendChild(themes); lookToggles.forEach(row => body.appendChild(toggle(row)))
    }
    if (step === 2) {
      p('The app lives in your tray. Closing this window hides it; choose Quit from the tray menu to exit.')
      notificationToggles.forEach(row => body.appendChild(toggle(row)))
      const label = document.createElement('strong'); label.className = 'onboarding-label'; label.textContent = 'Check every'; body.appendChild(label)
      const choices = document.createElement('div'); choices.className = 'onboarding-choices'
      ;[15,60,240,720].forEach(n => { const name = n < 60 ? n + ' min' : n / 60 + ' hr'; const b = button(name, 'onboarding-choice' + (settings.checkInterval === n ? ' active' : ''), () => { settings.checkInterval = n; render() }); b.dataset.focusKey = 'interval-' + n; b.setAttribute('aria-pressed', String(settings.checkInterval === n)); choices.appendChild(b) })
      body.appendChild(choices)
    }
    if (step === 3) {
      p("Sign-in is optional and enables wishlist alerts. You sign in on the store's own page; DealDrop keeps the sign-in only on this computer.")
      ;[['steam','Steam'],['gog','GOG'],['epic','Epic Games']].forEach(([key,name]) => {
        const row = document.createElement('div'); row.className = 'onboarding-account'
        const account = state.accounts[key], status = document.createElement('span')
        status.textContent = account ? name + ' — signed in as ' + (account.name || name + ' user') : name + ' — not signed in'
        if (account?.avatar) { const image = document.createElement('img'); image.src = account.avatar; image.alt = ''; row.appendChild(image) }
        row.appendChild(status)
        if (!account) { const signIn = button('Sign in', 'onboarding-secondary', async e => {
          dialog.querySelector('.onboarding-error').textContent = ''
          const b = e.currentTarget; b.disabled = true; b.textContent = 'Opening…'
          try { setAccounts(await window.api.login(key)); await api().loadData(); render() }
          catch { dialog.querySelector('.onboarding-error').textContent = 'Sign-in did not finish. You can try later in Settings.'; b.disabled = false; b.textContent = 'Try again' }
        }); signIn.dataset.focusKey = 'account-' + key; row.appendChild(signIn) }
        body.appendChild(row)
        if (key !== 'steam' && (state.launchers[key]?.options?.length ?? 0) > 1) {
          const pref = key === 'gog' ? 'launcherGog' : 'launcherEpic', label = document.createElement('label'); label.className = 'onboarding-pref'
          const shortName = name === 'Epic Games' ? 'Epic' : name
          label.textContent = 'Open ' + shortName + ' games in '
          const select = document.createElement('select'); select.setAttribute('aria-label', 'Open ' + shortName + ' games in')
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
      body.appendChild(button('Show me around', 'onboarding-secondary onboarding-tour-start', startTour))
    }
    const actions = dialog.querySelector('.onboarding-actions'); actions.replaceChildren()
    if (step) actions.appendChild(button('Back', 'onboarding-secondary', () => { step--; render() }))
    if (step < 4) { const skip = button('Skip', 'onboarding-skip', finish); skip.setAttribute('aria-label', 'Skip and keep my choices'); actions.appendChild(skip) }
    actions.appendChild(button(step === 4 ? 'Finish' : 'Next', 'onboarding-primary', () => { if (step === 4) finish(); else { step++; render() } }))
    const restore = focusKey && [...dialog.querySelectorAll('button:not(:disabled)[data-focus-key],select[data-focus-key],a[href][data-focus-key]')].find(el => el.dataset.focusKey === focusKey)
    dialog.querySelector('.onboarding-dialog').scrollTop = 0
    if (restore) restore.focus()
    else if (stepChanged || focusKey) dialog.querySelector('.onboarding-dialog button:not(:disabled),.onboarding-dialog select,.onboarding-dialog a[href]')?.focus()
  }
  function startTour () {
    if (!dialog || tourOverlay) return
    tourTrigger = document.activeElement
    dialog.classList.add('touring')
    dialog.querySelector('.onboarding-dialog').classList.add('tour-active')
    dialog.querySelector('.onboarding-dialog').setAttribute('aria-hidden', 'true')
    tourIndex = 0
    tourOverlay = document.createElement('div')
    tourOverlay.className = 'tour-backdrop'
    tourOverlay.innerHTML = '<div class="tour-spotlight" aria-hidden="true"></div><section class="tour-card" role="dialog" aria-modal="true" aria-labelledby="tour-title"><div class="tour-count" aria-live="polite" aria-atomic="true"></div><h2 id="tour-title"></h2><p class="tour-copy"></p><div class="tour-actions"></div></section>'
    document.body.appendChild(tourOverlay)
    addEventListener('resize', updateTourSpotlight)
    renderTour()
  }
  function endTour () {
    if (!tourOverlay) return
    removeEventListener('resize', updateTourSpotlight)
    tourOverlay.remove(); tourOverlay = null; tourIndex = -1
    dialog?.classList.remove('touring')
    const panel = dialog?.querySelector('.onboarding-dialog')
    panel?.classList.remove('tour-active'); panel?.removeAttribute('aria-hidden')
    if (tourTrigger?.isConnected) tourTrigger.focus()
  }
  function updateTourSpotlight () {
    if (!tourOverlay) return
    const stop = tourStops[tourIndex], target = stop.selector && document.querySelector(stop.selector)
    const spotlight = tourOverlay.querySelector('.tour-spotlight'), card = tourOverlay.querySelector('.tour-card')
    if (!target) {
      tourOverlay.classList.add('tour-note-only'); spotlight.hidden = true; card.classList.add('tour-card-centered'); card.classList.remove('tour-card-top')
      return
    }
    tourOverlay.classList.remove('tour-note-only'); spotlight.hidden = false; card.classList.remove('tour-card-centered')
    target.scrollIntoView({block:'center', inline:'nearest'})
    const sidebar = target.closest('.sidebar')
    const sidePlacement = sidebar && innerWidth - sidebar.getBoundingClientRect().right >= 280
    card.classList.toggle('tour-card-side', Boolean(sidePlacement))
    if (sidePlacement) card.classList.remove('tour-card-top')
    const rect = target.getBoundingClientRect()
    const left = Math.min(Math.max(4, rect.left - 6), innerWidth - 8)
    const top = Math.min(Math.max(4, rect.top - 6), innerHeight - 8)
    spotlight.style.left = left + 'px'
    spotlight.style.top = top + 'px'
    spotlight.style.width = Math.min(innerWidth - left - 4, rect.width + 12) + 'px'
    spotlight.style.height = Math.min(innerHeight - top - 4, rect.height + 12) + 'px'
    if (!sidePlacement) card.classList.toggle('tour-card-top', rect.top + rect.height / 2 > innerHeight * 0.55)
  }
  function renderTour () {
    const stop = tourStops[tourIndex]
    tourOverlay.querySelector('.tour-count').textContent = 'Tour ' + (tourIndex + 1) + ' of ' + tourStops.length
    tourOverlay.querySelector('#tour-title').textContent = stop.title
    tourOverlay.querySelector('.tour-copy').textContent = stop.copy
    const actions = tourOverlay.querySelector('.tour-actions'); actions.replaceChildren()
    if (tourIndex > 0) actions.appendChild(button('Back', 'onboarding-secondary', () => { tourIndex--; renderTour() }))
    actions.appendChild(button('Skip', 'onboarding-skip', endTour))
    actions.appendChild(button(tourIndex === tourStops.length - 1 ? 'Done' : 'Next', 'onboarding-primary', () => { if (tourIndex === tourStops.length - 1) endTour(); else { tourIndex++; renderTour() } }))
    updateTourSpotlight()
    actions.querySelector('button:last-child').focus()
  }
  window.startOnboarding = start
})()
