#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launchers;
mod sources;
mod stores;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sources::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use stores::Tokens;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::{Mutex as AsyncMutex, Notify};

// ─── Persistent data ──────────────────────────────────────────────────────────
// Same file and key names the Electron version used (~/.config/dealdrop/config.json),
// so existing users keep their keys, settings and Steam sign-in after upgrading.

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    notify_major_sale: bool,
    notify_free_game: bool,
    notify_free_weekend: bool,
    notify_wishlist_sale: bool,
    notify_big_deal: bool,
    check_interval: u64,
    /// Active theme; `base_theme` is the user's own pick to return to when a sale theme ends,
    /// `last_sale_theme` remembers which sale we already auto-switched for (so a restart
    /// doesn't undo the user switching away from it).
    theme: String,
    base_theme: String,
    last_sale_theme: String,
    rounded: bool,
    effects: bool,
    /// Where GOG / Epic links open: "auto" | "native" | "heroic" | "lutris" | "browser"
    launcher_gog: String,
    launcher_epic: String,
    /// The first-run tour has been completed or skipped.
    onboarded: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { notify_major_sale: true, notify_free_game: true, notify_free_weekend: true,
               notify_wishlist_sale: true, notify_big_deal: false, check_interval: 60,
               theme: "midnight".into(), base_theme: "midnight".into(), last_sale_theme: String::new(),
               rounded: false, effects: true, launcher_gog: "auto".into(), launcher_epic: "auto".into(), onboarded: false }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SteamProfile { steam_id: String, name: String, avatar: Option<String> }

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Db {
    settings: Settings,
    steam_id: Option<String>,
    steam_profile: Option<SteamProfile>,
    gog: Option<Tokens>,
    epic: Option<Tokens>,
    last_sale_key: Option<String>,
    seen_free_ids: HashSet<String>,
    seen_sale_ids: HashSet<String>,
    seen_deal_ids: HashSet<String>,
    /// The first poll has recorded the free list (absent in old config files, so false).
    primed: bool,
}

// ─── App state ────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    free_games: Vec<Free>,
    free_weekends: Vec<Free>,
    sales: Sales,
    live_sale: Option<LiveSale>,
    deals: Vec<Deal>,
    deals_source: String,
    deals_error: Option<String>,
    wishlist: Vec<WishItem>,
    wishlist_private: bool,
    wishlist_errors: HashMap<String, String>,
    /// Sources that failed this round ("Epic sales: HTTP 500"); the sidebar shows them in a red box.
    source_errors: Vec<String>,
    last_checked: u64,
}

struct AppState {
    path: PathBuf,
    db: Mutex<Db>,
    snapshot: Mutex<Option<Snapshot>>,
    poll_lock: AsyncMutex<()>,
    wake: Notify,
}

impl AppState {
    /// Mutate the db and persist it (0600: it holds API keys and tokens).
    fn edit<T>(&self, f: impl FnOnce(&mut Db) -> T) -> T {
        let mut db = self.db.lock().unwrap();
        let out = f(&mut db);
        if let Some(dir) = self.path.parent() { let _ = std::fs::create_dir_all(dir); }
        if let Ok(text) = serde_json::to_string_pretty(&*db) {
            let tmp = self.path.with_extension("tmp");
            // Created 0600 from the start (never briefly world-readable), then swapped in atomically.
            let mut o = std::fs::OpenOptions::new();
            o.write(true).create(true).truncate(true);
            #[cfg(unix)]
            { use std::os::unix::fs::OpenOptionsExt; o.mode(0o600); }
            if o.open(&tmp).and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes())).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
        out
    }
    fn read<T>(&self, f: impl FnOnce(&Db) -> T) -> T { f(&self.db.lock().unwrap()) }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Display name for a store/source id in notification text.
fn store_label(id: &str) -> &str {
    match id { "steam" => "Steam", "epic" => "Epic Games", "gog" => "GOG", "other" => "another store", o => o }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
}

/// Collapse "Foo" / "foo!" duplicates reported by both a store API and GamerPower.
fn dedupe_free(list: Vec<Free>) -> Vec<Free> {
    let mut seen = HashSet::new();
    list.into_iter().filter(|g| {
        let key: String = g.title.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        seen.insert(format!("{}:{key}", g.kind))
    }).collect()
}

// ─── Poll ─────────────────────────────────────────────────────────────────────

async fn poll(app: &AppHandle) -> Snapshot {
    let st = app.state::<AppState>();
    // One poll at a time; a second caller just waits for the first one's result.
    let Ok(_guard) = st.poll_lock.try_lock() else {
        let _ = st.poll_lock.lock().await;
        return st.snapshot.lock().unwrap().clone().unwrap_or_default();
    };
    let snap = run_poll(app, &st).await;
    *st.snapshot.lock().unwrap() = Some(snap.clone());
    let _ = app.emit("data-update", &snap);
    snap
}

/// "Epic sales: <reason>": not shown, only copied by the sidebar's "Copy error" button.
fn failure(name: &str, err: &str) -> String {
    format!("{name}: {}", err.chars().take(400).collect::<String>())
}

/// Fetch one wishlist store; on failure keep the last good items for that store.
fn keep_or_fail(res: R<Vec<WishItem>>, store: &str, prev: &Snapshot, errors: &mut HashMap<String, String>) -> Vec<WishItem> {
    match res {
        Ok(items) => items,
        Err(e) => {
            eprintln!("[wishlist:{store}] {e}");
            errors.insert(store.into(), e);
            prev.wishlist.iter().filter(|w| w.store == store).cloned().collect()
        }
    }
}

async fn run_poll(app: &AppHandle, st: &AppState) -> Snapshot {
    let (settings, steam_id, gog, epic) = st.read(|d| {
        (d.settings.clone(), d.steam_id.clone(), d.gog.clone(), d.epic.clone())
    });
    let prev = st.snapshot.lock().unwrap().clone().unwrap_or_default();

    let source_errors = Mutex::new(Vec::<String>::new());

    // 1. Steam major sale (always checked: it also drives the sale themes)
    let live_sale = match detect_steam_sale().await {
        Ok((on, live)) => {
            if on {
                let name = live.as_ref().map_or("Steam Sale".to_string(), |l| l.name.clone());
                if settings.notify_major_sale && st.read(|d| d.last_sale_key.as_deref() != Some(&name)) {
                    notify(app, "🎉 Steam sale started!", &format!("{name} is live — go grab some deals."));
                }
                st.edit(|d| d.last_sale_key = Some(name));
            } else {
                st.edit(|d| d.last_sale_key = None);
            }
            live
        }
        Err(e) => { eprintln!("[sale-detect] {e}"); source_errors.lock().unwrap().push(failure("Steam sale detection", &e)); prev.live_sale.clone() }
    };

    // 2. Everything that needs no account, in parallel
    // On failure keep the last good items from that source (matched by id prefix) instead of blanking them.
    let log = |name: &str, r: R<Vec<Free>>, old: &[Free], prefix: &str| r.unwrap_or_else(|e| {
        eprintln!("[{name}] {e}"); source_errors.lock().unwrap().push(failure(name, &e));
        // `steam-` is also a prefix of `steam-ftk-`: restoring featured must not swallow free-to-keep rows.
        old.iter().filter(|g| g.id.starts_with(prefix) && !(prefix == "steam-" && g.id.starts_with("steam-ftk-"))).cloned().collect()
    });
    let (ef, sf, sk, sw, gp, ss, gs, es, hot) = tokio::join!(
        epic_free(), steam_free_featured(), steam_free_to_keep(), steam_free_weekends(), gamerpower(),
        steam_sales(), gog_sales(), stores::epic_sales(), hot_deals(),
    );
    let (ef, sf, sk, sw, gp) = (
        log("Epic free games", ef, &prev.free_games, "epic-"), log("Steam free games", sf, &prev.free_games, "steam-"),
        log("Steam free to keep", sk, &prev.free_games, "steam-ftk-"),
        log("Steam free weekends", sw, &prev.free_weekends, "steam-fw-"), log("GamerPower giveaways", gp, &prev.free_games, "gp-"),
    );
    let deals_of = |name: &str, r: R<Vec<Deal>>, old: &Vec<Deal>| r.unwrap_or_else(|e| { eprintln!("[{name}] {e}"); source_errors.lock().unwrap().push(failure(name, &e)); old.clone() });
    let sales = Sales {
        steam: deals_of("Steam sales", ss, &prev.sales.steam),
        gog: deals_of("GOG sales", gs, &prev.sales.gog),
        epic: deals_of("Epic sales", es, &prev.sales.epic),
    };
    let (deals, deals_source, deals_error) = hot;

    let free_games = dedupe_free([ef, sf, sk, gp].concat());
    let free_weekends = dedupe_free(sw);

    // The very first poll on a fresh install only records what is free today; no notification burst.
    if st.read(|d| d.primed || !d.seen_free_ids.is_empty()) {
        if settings.notify_free_game { notify_new_free(app, st, &free_games, false); }
        if settings.notify_free_weekend { notify_new_free(app, st, &free_weekends, true); }
        if !st.read(|d| d.primed) { st.edit(|d| d.primed = true); }
    } else if source_errors.lock().unwrap().is_empty() {
        // Only prime from a complete baseline: a failed source would otherwise notify in a burst once it recovers.
        st.edit(|d| {
            d.primed = true;
            d.seen_free_ids.extend(free_games.iter().chain(free_weekends.iter()).map(|g| g.id.clone()));
        });
    }

    // 3. Wishlists
    let mut wishlist_errors = HashMap::new();
    let mut wishlist = vec![];
    let mut wishlist_private = false;
    if let Some(id) = &steam_id {
        let res = steam_wishlist(id).await;
        if matches!(&res, Ok(v) if v.is_empty()) { wishlist_private = steam_profile_private(id).await; }
        wishlist.extend(keep_or_fail(res, "steam", &prev, &mut wishlist_errors));
    }
    if let Some(t) = gog {
        let res = match fresh_tokens(app, st, t, true).await {
            Ok(t) => {
                if t.avatar.is_none() {
                    if let Some(a) = stores::gog_user(&t.access_token).await.3 { st.edit(|d| if let Some(g) = d.gog.as_mut() { g.avatar = Some(a) }); }
                }
                stores::gog_wishlist(&t.access_token).await
            }
            Err(e) => Err(e),
        };
        wishlist.extend(keep_or_fail(res, "gog", &prev, &mut wishlist_errors));
    }
    if let Some(t) = epic {
        let res = match fresh_tokens(app, st, t, false).await { Ok(t) => stores::epic_wishlist(&t.access_token).await, Err(e) => Err(e) };
        wishlist.extend(keep_or_fail(res, "epic", &prev, &mut wishlist_errors));
    }

    attach_cheapest(&mut wishlist).await;
    if settings.notify_wishlist_sale { notify_wishlist(app, st, &wishlist); }
    if settings.notify_big_deal { notify_big_deals(app, st, &deals); }

    Snapshot {
        free_games, free_weekends, sales, live_sale, deals, deals_source, deals_error,
        wishlist, wishlist_private, wishlist_errors, source_errors: source_errors.into_inner().unwrap(), last_checked: now_ms(),
    }
}

/// Refresh GOG/Epic tokens when they're about to expire and persist the result.
async fn fresh_tokens(_app: &AppHandle, st: &AppState, t: Tokens, gog: bool) -> R<Tokens> {
    if !t.stale() { return Ok(t); }
    let n = if gog { stores::gog_refresh(&t).await } else { stores::epic_refresh(&t).await }
        .map_err(|e| format!("{e} — sign in again"))?;
    let saved = n.clone();
    // Only while still signed in: a logout during the refresh must not sign the user back in.
    st.edit(|d| if gog { if d.gog.is_some() { d.gog = Some(saved) } } else if d.epic.is_some() { d.epic = Some(saved) });
    Ok(n)
}

async fn hot_deals() -> (Vec<Deal>, String, Option<String>) {
    match cheapshark_deals().await {
        Ok(d) => (d, "CheapShark".into(), None),
        Err(e) => (vec![], String::new(), Some(e)),
    }
}

fn notify_new_free(app: &AppHandle, st: &AppState, games: &[Free], weekend: bool) {
    let fresh: Vec<&Free> = st.read(|d| games.iter().filter(|g| !d.seen_free_ids.contains(&g.id)).collect());
    if fresh.is_empty() { return; }
    st.edit(|d| d.seen_free_ids.extend(fresh.iter().map(|g| g.id.clone())));
    if let [g] = fresh[..] {
        let title = if weekend { format!("Free weekend: {}", g.title) } else { format!("Free: {}", g.title) };
        notify(app, &title, &format!("{} is free on {}!", g.title, store_label(&g.source)));
    } else {
        let title = format!("{} free {} available", fresh.len(), if weekend { "weekend games" } else { "games" });
        notify(app, &title, &fresh.iter().map(|g| g.title.as_str()).collect::<Vec<_>>().join(", "));
    }
}

fn discount(w: &WishItem) -> u32 { w.price_info.as_ref().map_or(0, |p| p.discount) }

fn notify_wishlist(app: &AppHandle, st: &AppState, items: &[WishItem]) {
    let key = |w: &WishItem| format!("{}:{}", w.store, w.id);
    let fresh: Vec<&WishItem> = st.read(|d| items.iter().filter(|w| discount(w) >= 20 && !d.seen_sale_ids.contains(&key(w))).collect());
    if let [w] = fresh[..] {
        let p = w.price_info.as_ref().unwrap();
        notify(app, &format!("🏷️ {} is on sale!", w.name), &format!("{}% off — now {} on {}", p.discount, p.formatted.clone().unwrap_or_default(), store_label(&w.store)));
    } else if !fresh.is_empty() {
        let names: Vec<&str> = fresh.iter().take(4).map(|w| w.name.as_str()).collect();
        notify(app, &format!("{} wishlist games on sale", fresh.len()), &(names.join(", ") + if fresh.len() > 4 { "…" } else { "" }));
    }
    // Forget a game once it's no longer discounted so its next sale re-notifies
    st.edit(|d| {
        d.seen_sale_ids.extend(fresh.iter().map(|w| key(w)));
        for w in items { if discount(w) < 5 { d.seen_sale_ids.remove(&key(w)); } }
    });
}

fn notify_big_deals(app: &AppHandle, st: &AppState, deals: &[Deal]) {
    let fresh: Vec<&Deal> = st.read(|d| deals.iter().filter(|x| x.cut >= 75 && !d.seen_deal_ids.contains(&x.id)).collect());
    if let [d] = fresh[..] {
        notify(app, &format!("🔥 {}% off: {}", d.cut, d.title), &format!("{} — ${:.2}", d.shop, d.price));
    } else if !fresh.is_empty() {
        let names: Vec<&str> = fresh.iter().take(4).map(|d| d.title.as_str()).collect();
        notify(app, &format!("{} deals over 75% off", fresh.len()), &(names.join(", ") + if fresh.len() > 4 { "…" } else { "" }));
    }
    // Hysteresis: forget below 60% so flicker doesn't re-notify but a re-deepened deal does
    st.edit(|s| {
        s.seen_deal_ids.extend(fresh.iter().map(|d| d.id.clone()));
        for d in deals { if d.cut < 60 { s.seen_deal_ids.remove(&d.id); } }
    });
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Acct { name: String, avatar: Option<String>, profile_url: Option<String> }

#[derive(Serialize)]
struct Accounts { steam: Option<Acct>, gog: Option<Acct>, epic: Option<Acct> }

fn accounts(st: &AppState) -> Accounts {
    st.read(|d| Accounts {
        steam: d.steam_profile.as_ref().map(|p| Acct { name: p.name.clone(), avatar: p.avatar.clone(),
            profile_url: Some(format!("https://steamcommunity.com/profiles/{}", p.steam_id)) }),
        gog: d.gog.as_ref().map(|t| Acct { name: t.name.clone(), avatar: t.avatar.clone(),
            profile_url: url::Url::parse("https://www.gog.com/u/").and_then(|u| u.join(&t.name)).ok().map(String::from) }),
        // Epic has no public profile page; its account page is the closest thing.
        epic: d.epic.as_ref().map(|t| Acct { name: t.name.clone(), avatar: t.avatar.clone(),
            profile_url: Some("https://www.epicgames.com/account/personal".into()) }),
    })
}

#[tauri::command]
fn get_settings(st: State<AppState>) -> Settings { st.read(|d| d.settings.clone()) }

#[tauri::command]
fn save_settings(st: State<AppState>, settings: Settings) {
    let changed = st.edit(|d| { let c = d.settings.check_interval != settings.check_interval; d.settings = settings; c });
    if changed { st.wake.notify_one(); }
}

#[tauri::command]
async fn fetch_data(app: AppHandle) -> Snapshot {
    let cached = app.state::<AppState>().snapshot.lock().unwrap().clone();
    match cached { Some(s) => s, None => poll(&app).await }
}

#[tauri::command]
async fn check_now(app: AppHandle) -> Snapshot { poll(&app).await }

#[tauri::command]
fn get_accounts(st: State<AppState>) -> Accounts { accounts(&st) }

#[tauri::command]
async fn steam_login(app: AppHandle) -> Result<Accounts, String> {
    let st = app.state::<AppState>();
    let id = stores::steam_login(&app).await?;
    let (name, avatar) = steam_profile(&id).await;
    st.edit(|d| {
        d.steam_id = Some(id.clone());
        d.steam_profile = Some(SteamProfile { steam_id: id, name, avatar });
        d.seen_sale_ids.clear();
    });
    *st.snapshot.lock().unwrap() = None;
    Ok(accounts(&st))
}

#[tauri::command]
async fn gog_login(app: AppHandle) -> Result<Accounts, String> {
    let st = app.state::<AppState>();
    let t = stores::gog_login(&app).await?;
    st.edit(|d| d.gog = Some(t));
    *st.snapshot.lock().unwrap() = None;
    Ok(accounts(&st))
}

#[tauri::command]
async fn epic_login(app: AppHandle) -> Result<Accounts, String> {
    let st = app.state::<AppState>();
    let t = stores::epic_login(&app).await?;
    st.edit(|d| d.epic = Some(t));
    *st.snapshot.lock().unwrap() = None;
    Ok(accounts(&st))
}

#[tauri::command]
fn logout(st: State<AppState>, store: String) -> Accounts {
    st.edit(|d| {
        match store.as_str() {
            "steam" => { d.steam_id = None; d.steam_profile = None; }
            "gog" => d.gog = None,
            _ => d.epic = None,
        }
        d.seen_sale_ids.retain(|k| !k.starts_with(&format!("{store}:")));
    });
    if let Some(s) = st.snapshot.lock().unwrap().as_mut() { s.wishlist.retain(|w| w.store != store); }
    accounts(&st)
}

fn open_raw(app: &AppHandle, url: &str) -> bool { app.opener().open_url(url, None::<&str>).is_ok() }

/// Only plain `https` links to a real hostname: no other schemes (file:, javascript:, custom),
/// no embedded credentials (`https://steampowered.com@evil.example`), no IP literals or localhost.
fn safe_https(raw: &str) -> Option<url::Url> {
    let u = url::Url::parse(raw).ok()?;
    let host_ok = matches!(u.host(), Some(url::Host::Domain(d)) if d != "localhost" && d.contains('.'));
    (u.scheme() == "https" && host_ok && u.username().is_empty() && u.password().is_none()).then_some(u)
}

/// Opens in the user's default browser (never an in-app webview).
#[tauri::command]
fn open_url(app: AppHandle, url: String) -> bool {
    safe_https(&url).is_some_and(|u| open_raw(&app, u.as_str()))
}

#[tauri::command]
fn launchers(st: State<AppState>) -> HashMap<&'static str, launchers::StoreLaunchers> {
    let (gog, epic) = st.read(|d| (d.settings.launcher_gog.clone(), d.settings.launcher_epic.clone()));
    HashMap::from([
        ("steam", launchers::resolve("steam", "auto")),
        ("epic", launchers::resolve("epic", &epic)),
        ("gog", launchers::resolve("gog", &gog)),
    ])
}

/// Open a store page in the preferred installed launcher, falling back to the browser.
#[tauri::command]
fn open_in_app(app: AppHandle, store: String, url: String, id: Option<String>) -> bool {
    let Some(parsed) = safe_https(&url) else { return false };
    let pref = app.state::<AppState>().read(|d| match store.as_str() {
        "gog" => d.settings.launcher_gog.clone(),
        "epic" => d.settings.launcher_epic.clone(),
        _ => "auto".into(),
    });
    let chosen = launchers::resolve(&store, &pref).chosen;
    chosen.and_then(|c| launchers::link(&store, c.key, &parsed, id.as_deref()))
        .is_some_and(|l| open_raw(&app, &l))
        || open_raw(&app, parsed.as_str())
}

#[tauri::command]
fn window_minimize(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.minimize(); } }

#[tauri::command]
fn window_close(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); } }

#[tauri::command]
async fn wishlist_game_deals(app_id: String, name: String) -> Value {
    let ids = [app_id.clone()];
    let (steam, cheapest) = tokio::join!(steam_items(&ids), cheapest_for_steam_app(&app_id));
    let steam = steam.ok().and_then(|m| m.get(&app_id).and_then(|x| x.price.clone())).map(|p| json!({
        "onSale": p.discount > 0, "discount": p.discount, "final": p.fin, "initial": p.initial, "formatted": p.formatted,
    }));
    json!({ "appId": app_id, "gameName": name, "steam": steam, "cheapest": cheapest.ok().flatten(), "checkedAt": now_ms() })
}

// ─── Boot ─────────────────────────────────────────────────────────────────────

fn load_db(path: &PathBuf) -> Db {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let path = app.path().config_dir()?.join("dealdrop").join("config.json");
            app.manage(AppState {
                db: Mutex::new(load_db(&path)),
                path,
                snapshot: Mutex::new(None),
                poll_lock: AsyncMutex::new(()),
                wake: Notify::new(),
            });

            // The main webview may only ever show the bundled UI. Anything else (a stray link,
            // a redirect) is blocked; real links go through `open_url` to the default browser.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("DealDrop")
                .inner_size(980.0, 660.0)
                .min_inner_size(570.0, 420.0)
                .decorations(false)
                .transparent(true) // the page paints the background so the window corners can be rounded
                .on_navigation(|u| matches!(u.scheme(), "tauri" | "about") || u.host_str() == Some("tauri.localhost"))
                .build()?;

            let menu = Menu::with_items(app, &[
                &MenuItem::with_id(app, "open", "Open DealDrop", true, None::<&str>)?,
                &MenuItem::with_id(app, "check", "Refresh all", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
            ])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().ok_or("no icon")?)
                .tooltip("DealDrop")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "open" => show_main(app),
                    "check" => { let a = app.clone(); tauri::async_runtime::spawn(async move { poll(&a).await; }); }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        let app = tray.app_handle();
                        match app.get_webview_window("main") {
                            Some(w) if w.is_visible().unwrap_or(false) => { let _ = w.hide(); }
                            _ => show_main(app),
                        }
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    poll(&handle).await;
                    let st = handle.state::<AppState>();
                    let mins = st.read(|d| d.settings.check_interval).max(5);
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(mins * 60)) => {}
                        _ = st.wake.notified() => {}
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|w, ev| {
            // Close button hides to tray; Quit lives in the tray menu
            if let (WindowEvent::CloseRequested { api, .. }, "main") = (ev, w.label()) {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings, save_settings, fetch_data, check_now, get_accounts,
            steam_login, gog_login, epic_login, logout, open_url, open_in_app, launchers,
            window_minimize, window_close, wishlist_game_deals,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DealDrop");
}

#[cfg(test)]
mod tests {
    use super::{dedupe_free, failure, safe_https, store_label, Acct, Free};

    #[test]
    fn account_json_is_camel_case() {
        let v = serde_json::to_value(Acct { name: "n".into(), avatar: None, profile_url: Some("https://x.y".into()) }).unwrap();
        assert_eq!(v["profileUrl"], "https://x.y"); // the UI reads profileUrl
    }

    #[test]
    fn store_names_for_notifications() {
        assert_eq!(store_label("steam"), "Steam");
        assert_eq!(store_label("epic"), "Epic Games");
        assert_eq!(store_label("gog"), "GOG");
        assert_eq!(store_label("other"), "another store");
        assert_eq!(store_label("itch"), "itch");
    }

    #[test]
    fn dedupe_ignores_case_and_punctuation_but_not_kind() {
        let free = |id: &str, title: &str, kind: &str| Free { id: id.into(), title: title.into(), kind: kind.into(), ..Default::default() };
        let out = dedupe_free(vec![
            free("epic-1", "Foo!", "free"), free("gp-2", "foo", "free"), free("steam-fw-3", "Foo", "weekend"),
        ]);
        assert_eq!(out.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["epic-1", "steam-fw-3"]);
    }

    #[test]
    fn failure_text_is_capped() {
        let msg = failure("Epic sales", &"x".repeat(1000));
        assert_eq!(msg.chars().count(), "Epic sales: ".len() + 400);
    }

    #[test]
    fn only_plain_https_hostnames() {
        assert!(safe_https("https://store.steampowered.com/app/60").is_some());
        for bad in ["http://store.steampowered.com", "file:///etc/passwd", "javascript:alert(1)", "steam://run/1",
                    "https://store.steampowered.com@evil.example/", "https://127.0.0.1/", "https://localhost/", "https://[::1]/", "https://intranet/"] {
            assert!(safe_https(bad).is_none(), "{bad} should be rejected");
        }
    }
}
