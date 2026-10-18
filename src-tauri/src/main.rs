#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
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
    wishlist_min_cut: u32,
    notify_big_deal: bool,
    big_deal_min_cut: u32,
    check_interval: u64,
    /// Active theme; `base_theme` is the user's own pick to return to when a sale theme ends,
    /// `last_sale_theme` remembers which sale we already auto-switched for (so a restart
    /// doesn't undo the user switching away from it).
    theme: String,
    base_theme: String,
    last_sale_theme: String,
    rounded: bool,
    effects: bool,
    /// Where GOG / Epic links open: "auto" | "native" | "browser"; old GOG preferences fall back to the first available option.
    launcher_gog: String,
    launcher_epic: String,
    /// The first-run tour has been completed or skipped.
    onboarded: bool,
    hidden: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self { notify_major_sale: true, notify_free_game: true, notify_free_weekend: true,
               notify_wishlist_sale: true, wishlist_min_cut: 20, notify_big_deal: false, big_deal_min_cut: 75, check_interval: 60,
               theme: "midnight".into(), base_theme: "midnight".into(), last_sale_theme: String::new(),
               rounded: false, effects: true, launcher_gog: "auto".into(), launcher_epic: "auto".into(), onboarded: false, hidden: Vec::new() }
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
                // mode() is ignored when config.tmp already exists; rename would publish that mode.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).is_err() {
                        let _ = std::fs::remove_file(&tmp);
                        return out;
                    }
                }
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

// ─── Tray ─────────────────────────────────────────────────────────────────────

/// Tray menu shortcuts whose labels carry the latest counts.
struct TrayItems { free: MenuItem<tauri::Wry>, wishlist: MenuItem<tauri::Wry> }

/// Free games + free weekends, and wishlist games at least 20% off.
/// Keep tray counts and sidebar badges fixed at 20%; only notifications use the preference.
fn tray_counts(s: &Snapshot) -> (usize, usize) {
    (s.free_games.len() + s.free_weekends.len(), s.wishlist.iter().filter(|w| discount(w) >= 20).count())
}

/// "DealDrop: 3 free games · 1 wishlist game on sale"; zero parts are left out.
fn tray_tooltip(free: usize, sale: usize) -> String {
    let mut parts = vec![];
    if free > 0 { parts.push(format!("{free} free game{}", if free == 1 { "" } else { "s" })); }
    if sale > 0 { parts.push(format!("{sale} wishlist game{} on sale", if sale == 1 { "" } else { "s" })); }
    if parts.is_empty() { "DealDrop".into() } else { format!("DealDrop: {}", parts.join(" · ")) }
}

/// "Free now (3)", or just "Free now" at zero.
fn with_count(label: &str, n: usize) -> String {
    if n > 0 { format!("{label} ({n})") } else { label.into() }
}

fn update_tray(app: &AppHandle, snap: &Snapshot) {
    let (free, sale) = tray_counts(snap);
    if let Some(t) = app.tray_by_id("main") { let _ = t.set_tooltip(Some(tray_tooltip(free, sale))); }
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.free.set_text(with_count("Free now", free));
        let _ = items.wishlist.set_text(with_count("Wishlist", sale));
    }
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
    update_tray(app, &snap);
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

/// Last-good rows for one source. `steam-` is also a prefix of `steam-ftk-`, so a featured
/// restore must not swallow free-to-keep rows. Any other id with that prefix is kept (`steam-fw-` stays).
fn restore_by_prefix(old: &[Free], prefix: &str) -> Vec<Free> {
    old.iter().filter(|g| g.id.starts_with(prefix) && !(prefix == "steam-" && g.id.starts_with("steam-ftk-"))).cloned().collect()
}

/// First poll: `Notify` is the normal path (`mark` records an upgrade from a config that
/// already has seen ids). `Record` seeds silently. `Wait` leaves a failed first poll unprimed.
enum Prime { Notify { mark: bool }, Record, Wait }

fn should_prime(primed: bool, seen_empty: bool, had_errors: bool) -> Prime {
    if primed || !seen_empty { Prime::Notify { mark: !primed } }
    else if had_errors { Prime::Wait }
    else { Prime::Record }
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
    // Only a failed free-game source blocks the silent first seed. A sales or sale-detect
    // error is not part of that baseline, so it must not hold off free-game notifications.
    let free_failed = Mutex::new(false);
    let log = |name: &str, r: R<Vec<Free>>, old: &[Free], prefix: &str| r.unwrap_or_else(|e| {
        eprintln!("[{name}] {e}"); source_errors.lock().unwrap().push(failure(name, &e));
        *free_failed.lock().unwrap() = true;
        restore_by_prefix(old, prefix)
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
    let had_errors = *free_failed.lock().unwrap();
    match st.read(|d| should_prime(d.primed, d.seen_free_ids.is_empty(), had_errors)) {
        Prime::Notify { mark } => {
            if settings.notify_free_game { notify_new_free(app, st, &free_games, false); }
            if settings.notify_free_weekend { notify_new_free(app, st, &free_weekends, true); }
            if mark { st.edit(|d| d.primed = true); }
        }
        Prime::Record => {
            // Only prime from a complete baseline: a failed source would otherwise notify in a burst once it recovers.
            st.edit(|d| {
                d.primed = true;
                d.seen_free_ids.extend(free_games.iter().chain(free_weekends.iter()).map(|g| g.id.clone()));
            });
        }
        Prime::Wait => {} // errors on the first poll: leave it unprimed so a later clean poll can seed
    }

    // 3. Wishlists. Drop a store's rows when that sign-in changed during this poll:
    // Ok(empty) so a failure cannot restore the previous account's wishlist.
    let mut wishlist_errors = HashMap::new();
    let mut wishlist = vec![];
    let mut wishlist_private = false;
    if let Some(id) = &steam_id {
        let res = steam_wishlist(id).await;
        if st.read(|d| d.steam_id.as_deref() == Some(id.as_str())) && matches!(&res, Ok(v) if v.is_empty()) {
            let private = steam_profile_private(id).await;
            if st.read(|d| d.steam_id.as_deref() == Some(id.as_str())) { wishlist_private = private; }
        }
        let still = st.read(|d| d.steam_id.as_deref() == Some(id.as_str()));
        if !still { wishlist_private = false; }
        let res = if still { res } else { Ok(vec![]) };
        wishlist.extend(keep_or_fail(res, "steam", &prev, &mut wishlist_errors));
    }
    if let Some(t) = gog {
        let started = t.refresh_token.clone();
        let res = match fresh_tokens(app, st, t, true).await {
            Ok(t) if same_refresh(st, true, &t.refresh_token) => {
                if t.avatar.is_none() {
                    if let Some(a) = stores::gog_user(&t.access_token).await.3 {
                        st.edit(|d| if let Some(g) = d.gog.as_mut() { if g.refresh_token == t.refresh_token { g.avatar = Some(a) } });
                    }
                }
                let res = if same_refresh(st, true, &t.refresh_token) { stores::gog_wishlist(&t.access_token).await } else { Ok(vec![]) };
                if same_refresh(st, true, &t.refresh_token) { res } else { Ok(vec![]) }
            }
            Ok(_) => Ok(vec![]),
            Err(e) if same_refresh(st, true, &started) => Err(e),
            Err(_) => Ok(vec![]),
        };
        wishlist.extend(keep_or_fail(res, "gog", &prev, &mut wishlist_errors));
    }
    if let Some(t) = epic {
        let started = t.refresh_token.clone();
        let res = match fresh_tokens(app, st, t, false).await {
            Ok(t) if same_refresh(st, false, &t.refresh_token) => {
                let res = stores::epic_wishlist(&t.access_token).await;
                if same_refresh(st, false, &t.refresh_token) { res } else { Ok(vec![]) }
            }
            Ok(_) => Ok(vec![]),
            Err(e) if same_refresh(st, false, &started) => Err(e),
            Err(_) => Ok(vec![]),
        };
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

/// True when this store's slot still holds `refresh` (the token this poll is acting for).
fn same_refresh(st: &AppState, gog: bool, refresh: &str) -> bool {
    st.read(|d| {
        let slot = if gog { &d.gog } else { &d.epic };
        slot.as_ref().is_some_and(|s| s.refresh_token == refresh)
    })
}

/// Refresh GOG/Epic tokens when they're about to expire and persist the result.
/// Writes only if the slot still holds the refresh token this call started with, so a logout
/// or a newer sign-in is not overwritten by the in-flight refresh.
async fn fresh_tokens(_app: &AppHandle, st: &AppState, t: Tokens, gog: bool) -> R<Tokens> {
    if !t.stale() { return Ok(t); }
    let n = if gog { stores::gog_refresh(&t).await } else { stores::epic_refresh(&t).await }
        .map_err(|e| format!("{e} — sign in again"))?;
    let saved = n.clone();
    st.edit(|d| {
        let slot = if gog { &mut d.gog } else { &mut d.epic };
        let same = slot.as_ref().is_some_and(|s| s.refresh_token == t.refresh_token);
        if same { *slot = Some(saved); }
    });
    Ok(n)
}

async fn hot_deals() -> (Vec<Deal>, String, Option<String>) {
    match cheapshark_deals().await {
        Ok(d) => (d, "CheapShark".into(), None),
        Err(e) => (vec![], String::new(), Some(e)),
    }
}

struct Notice { title: String, body: String }

/// Hidden games match by title, ignoring case and spacing, so they match across stores.
fn normalize_title(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn is_hidden(hidden: &[String], title: &str) -> bool {
    let key = normalize_title(title);
    hidden.iter().any(|h| normalize_title(h) == key)
}

/// Titles as shown (spacing tidied), without duplicates; only the newest 500 are kept.
fn normalize_hidden(hidden: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for title in hidden {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if !title.is_empty() && !is_hidden(&out, &title) { out.push(title); }
    }
    if out.len() > 500 { out.drain(..out.len() - 500); }
    out
}

/// Ids already in `seen` have been notified.
fn fresh_free<'a>(items: &'a [Free], seen: &HashSet<String>, hidden: &[String]) -> Vec<&'a Free> {
    items.iter().filter(|g| !seen.contains(&g.id) && !is_hidden(hidden, &g.title)).collect()
}

/// One game gets its own notice. Several share one, listing every title.
fn free_notice(fresh: &[&Free], weekend: bool) -> Option<Notice> {
    match fresh {
        [] => None,
        [g] => Some(Notice {
            title: if weekend { format!("Free weekend: {}", g.title) } else { format!("Free: {}", g.title) },
            body: format!("{} is free on {}!", g.title, store_label(&g.source)),
        }),
        many => Some(Notice {
            title: format!("{} free {} available", many.len(), if weekend { "weekend games" } else { "games" }),
            body: many.iter().map(|g| g.title.as_str()).collect::<Vec<_>>().join(", "),
        }),
    }
}

fn notify_new_free(app: &AppHandle, st: &AppState, games: &[Free], weekend: bool) {
    let fresh = st.read(|d| fresh_free(games, &d.seen_free_ids, &d.settings.hidden));
    if fresh.is_empty() { return; }
    st.edit(|d| d.seen_free_ids.extend(fresh.iter().map(|g| g.id.clone())));
    if let Some(n) = free_notice(&fresh, weekend) { notify(app, &n.title, &n.body); }
}

fn discount(w: &WishItem) -> u32 { w.price_info.as_ref().map_or(0, |p| p.discount) }

fn clamp_cut(value: u32, allowed: &[u32]) -> u32 {
    *allowed.iter().min_by_key(|&&cut| (cut.abs_diff(value), cut)).unwrap()
}

fn sale_key(w: &WishItem) -> String { format!("{}:{}", w.store, w.id) }

/// At the configured cut or more, and not already notified for this sale.
fn fresh_wishlist<'a>(items: &'a [WishItem], seen: &HashSet<String>, hidden: &[String], min_cut: u32) -> Vec<&'a WishItem> {
    items.iter().filter(|w| discount(w) >= min_cut && !seen.contains(&sale_key(w)) && !is_hidden(hidden, &w.name)).collect()
}

/// One game names its price. Several share one notice, the first four names, then an ellipsis.
fn wishlist_notice(fresh: &[&WishItem]) -> Option<Notice> {
    match fresh {
        [] => None,
        [w] => {
            let p = w.price_info.as_ref().unwrap();
            Some(Notice {
                title: format!("🏷️ {} is on sale!", w.name),
                body: format!("{}% off — now {} on {}", p.discount, p.formatted.clone().unwrap_or_default(), store_label(&w.store)),
            })
        }
        many => {
            let names: Vec<&str> = many.iter().take(4).map(|w| w.name.as_str()).collect();
            Some(Notice {
                title: format!("{} wishlist games on sale", many.len()),
                body: names.join(", ") + if many.len() > 4 { "…" } else { "" },
            })
        }
    }
}

/// Record what we just notified. Forget a game once its discount falls under 5% so the next
/// sale notifies again. A deeper cut while it stays at 5% or more does not.
fn remember_wishlist(seen: &mut HashSet<String>, items: &[WishItem], fresh: &[&WishItem]) {
    seen.extend(fresh.iter().map(|w| sale_key(w)));
    for w in items { if discount(w) < 5 { seen.remove(&sale_key(w)); } }
}

fn notify_wishlist(app: &AppHandle, st: &AppState, items: &[WishItem]) {
    let fresh = st.read(|d| fresh_wishlist(items, &d.seen_sale_ids, &d.settings.hidden, d.settings.wishlist_min_cut));
    if let Some(n) = wishlist_notice(&fresh) { notify(app, &n.title, &n.body); }
    st.edit(|d| remember_wishlist(&mut d.seen_sale_ids, items, &fresh));
}

fn fresh_big_deals<'a>(items: &'a [Deal], seen: &HashSet<String>, hidden: &[String], min_cut: u32) -> Vec<&'a Deal> {
    items.iter().filter(|x| x.cut >= min_cut && !seen.contains(&x.id) && !is_hidden(hidden, &x.title)).collect()
}

fn notify_big_deals(app: &AppHandle, st: &AppState, deals: &[Deal]) {
    let (fresh, min_cut): (Vec<&Deal>, u32) = st.read(|d| (fresh_big_deals(deals, &d.seen_deal_ids, &d.settings.hidden, d.settings.big_deal_min_cut), d.settings.big_deal_min_cut));
    if let [d] = fresh[..] {
        notify(app, &format!("🔥 {}% off: {}", d.cut, d.title), &format!("{} — ${:.2}", d.shop, d.price));
    } else if !fresh.is_empty() {
        let names: Vec<&str> = fresh.iter().take(4).map(|d| d.title.as_str()).collect();
        notify(app, &format!("{} deals over {}% off", fresh.len(), min_cut), &(names.join(", ") + if fresh.len() > 4 { "…" } else { "" }));
    }
    // Hysteresis stays 15 points below the selected notification threshold.
    st.edit(|s| remember_big_deals(&mut s.seen_deal_ids, deals, &fresh, min_cut));
}

fn remember_big_deals(seen: &mut HashSet<String>, deals: &[Deal], fresh: &[&Deal], min_cut: u32) {
    seen.extend(fresh.iter().map(|d| d.id.clone()));
    for d in deals { if d.cut < min_cut.saturating_sub(15) { seen.remove(&d.id); } }
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
    let mut settings = settings;
    settings.hidden = normalize_hidden(settings.hidden);
    settings.wishlist_min_cut = clamp_cut(settings.wishlist_min_cut, &[10, 20, 30, 50, 75]);
    settings.big_deal_min_cut = clamp_cut(settings.big_deal_min_cut, &[50, 60, 75, 85, 90]);
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
async fn open_deal_in_app(app: AppHandle, store: String, title: String, fallback_url: String) -> bool {
    let pref = app.state::<AppState>().read(|d| match store.as_str() {
        "gog" => d.settings.launcher_gog.clone(),
        "epic" => d.settings.launcher_epic.clone(),
        _ => "browser".into(),
    });
    let chosen = launchers::resolve(&store, &pref).chosen;
    if let Some(chosen) = chosen {
        let target = match store.as_str() {
            "gog" => sources::gog_product_for_title(&title).await.ok().flatten().map(|id|
                ("https://www.gog.com/".into(), Some(id))),
            "epic" => stores::epic_product_for_title(&title).await.ok().flatten().map(|slug|
                (format!("https://store.epicgames.com/en-US/p/{slug}"), None)),
            _ => None,
        };
        if let Some((url, id)) = target {
            if let Some(parsed) = safe_https(&url) {
                if let Some(link) = launchers::link(&store, chosen.key, &parsed, id.as_deref()) {
                    if open_raw(&app, &link) { return true; }
                }
            }
        }
    }
    safe_https(&fallback_url).is_some_and(|u| open_raw(&app, u.as_str()))
}

#[tauri::command]
fn window_minimize(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.minimize(); } }

#[tauri::command]
fn window_close(app: AppHandle) { if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); } }

#[tauri::command]
fn get_autostart() -> bool { autostart::get() }

#[tauri::command]
fn set_autostart(on: bool) -> Result<(), String> { autostart::set(on) }

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
        // A second launch shows the window, unless it is the login autostart (`--hidden`)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _| {
            if !argv.iter().any(|a| a == autostart::HIDDEN) { show_main(app) }
        }))
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
                .visible(!std::env::args().any(|a| a == autostart::HIDDEN)) // login autostart: tray only
                // `cargo tauri dev` serves the UI from http://127.0.0.1:1430; release builds never do.
                .on_navigation(|u| matches!(u.scheme(), "tauri" | "about") || u.host_str() == Some("tauri.localhost")
                    || (cfg!(debug_assertions) && u.scheme() == "http" && matches!(u.host_str(), Some("127.0.0.1" | "localhost"))))
                .build()?;

            let items = TrayItems {
                free: MenuItem::with_id(app, "free", "Free now", true, None::<&str>)?,
                wishlist: MenuItem::with_id(app, "wishlist", "Wishlist", true, None::<&str>)?,
            };
            let menu = Menu::with_items(app, &[
                &MenuItem::with_id(app, "open", "Open DealDrop", true, None::<&str>)?,
                &items.free,
                &items.wishlist,
                &MenuItem::with_id(app, "check", "Refresh all", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
            ])?;
            app.manage(items);
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().ok_or("no icon")?)
                .tooltip("DealDrop")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "open" => show_main(app),
                    // Page names the UI accepts from `navigate` (see `onNavigate` in app.js)
                    page @ ("free" | "wishlist") => { show_main(app); let _ = app.emit("navigate", page); }
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
            steam_login, gog_login, epic_login, logout, open_url, open_in_app, open_deal_in_app, launchers,
            window_minimize, window_close, wishlist_game_deals, get_autostart, set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DealDrop");
}

#[cfg(test)]
mod tests {
    use super::{clamp_cut, dedupe_free, failure, free_notice, fresh_big_deals, fresh_free, fresh_wishlist, is_hidden, normalize_hidden, normalize_title, remember_big_deals, remember_wishlist, restore_by_prefix, safe_https, should_prime, store_label, tray_counts, tray_tooltip, with_count, wishlist_notice, Acct, Deal, Free, Price, Prime, Settings, WishItem};
    use std::collections::HashSet;

    #[test]
    fn tray_text_counts_like_the_badges() {
        assert_eq!(tray_tooltip(0, 0), "DealDrop");
        assert_eq!(tray_tooltip(3, 5), "DealDrop: 3 free games · 5 wishlist games on sale");
        assert_eq!(tray_tooltip(1, 0), "DealDrop: 1 free game");
        assert_eq!(tray_tooltip(0, 1), "DealDrop: 1 wishlist game on sale");
        assert_eq!(with_count("Free now", 3), "Free now (3)");
        assert_eq!(with_count("Wishlist", 0), "Wishlist");
        let wish = |discount| WishItem { price_info: Some(Price { discount, ..Default::default() }), ..Default::default() };
        let snap = super::Snapshot {
            free_games: vec![Free::default(), Free::default()],
            free_weekends: vec![Free::default()],
            wishlist: vec![wish(20), wish(19), WishItem::default(), wish(75)],
            ..Default::default()
        };
        assert_eq!(tray_counts(&snap), (3, 2));
    }

    #[test]
    fn hidden_settings_default_empty_and_serialize_camel_case() {
        let settings: Settings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(settings.hidden.is_empty());
        assert!(serde_json::to_value(settings).unwrap().get("hidden").is_some());
    }

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

    #[test]
    fn restore_by_prefix_splits_steam_ftk() {
        let free = |id: &str| Free { id: id.into(), ..Default::default() };
        let old = vec![free("steam-10"), free("steam-ftk-10"), free("steam-fw-10"), free("gp-400"), free("epic-1")];
        let ids = |v: Vec<Free>| v.into_iter().map(|g| g.id).collect::<Vec<_>>();
        assert_eq!(ids(restore_by_prefix(&old, "steam-")), ["steam-10", "steam-fw-10"]);
        assert_eq!(ids(restore_by_prefix(&old, "steam-ftk-")), ["steam-ftk-10"]);
        assert_eq!(ids(restore_by_prefix(&old, "gp-")), ["gp-400"]);
    }

    #[test]
    fn should_prime_waits_on_errors_and_notifies_on_upgrade() {
        assert!(matches!(should_prime(false, true, true), Prime::Wait));
        assert!(matches!(should_prime(false, true, false), Prime::Record));
        // primed=false but seen ids already stored (upgrade): notify, and mark primed, errors or not.
        assert!(matches!(should_prime(false, false, true), Prime::Notify { mark: true }));
        assert!(matches!(should_prime(false, false, false), Prime::Notify { mark: true }));
        assert!(matches!(should_prime(true, true, true), Prime::Notify { mark: false }));
        assert!(matches!(should_prime(true, false, false), Prime::Notify { mark: false }));
    }

    fn game(id: &str, title: &str, source: &str, kind: &str) -> Free {
        Free { id: id.into(), title: title.into(), source: source.into(), kind: kind.into(), ..Default::default() }
    }

    fn wish(id: &str, name: &str, off: Option<u32>) -> WishItem {
        WishItem {
            store: "steam".into(), id: id.into(), name: name.into(),
            price_info: off.map(|discount| Price { discount, formatted: Some("$4.99".into()), ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn hidden_titles_normalize_and_keep_only_the_newest_500() {
        assert_eq!(normalize_title("  The\t  LONG\nWay Home "), "the long way home");
        assert_eq!(normalize_hidden(vec![" HADES ".into(), "hades".into(), "Control".into()]), ["HADES", "Control"]);
        assert!(is_hidden(&["Hades  II".into()], "hades ii") && !is_hidden(&["Hades".into()], "Hades II"));
        let hidden = normalize_hidden((0..501).map(|i| format!(" Game {i} ")).collect());
        assert_eq!(hidden.len(), 500);
        assert_eq!(hidden.first().unwrap(), "Game 1");
        assert_eq!(hidden.last().unwrap(), "Game 500");
    }

    #[test]
    fn hidden_titles_suppress_all_notification_types() {
        let hidden = vec!["HADES".to_string()];
        let free = vec![game("steam-1", " Hades ", "steam", "free"), game("steam-2", "Control", "steam", "free")];
        assert_eq!(fresh_free(&free, &HashSet::new(), &hidden).iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["steam-2"]);
        let wishlist = vec![wish("1", "Hades", Some(40)), wish("2", "Control", Some(40))];
        assert_eq!(fresh_wishlist(&wishlist, &HashSet::new(), &hidden, 20).iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["2"]);
        let deals = vec![Deal { id: "1".into(), title: "Hades".into(), cut: 90, ..Default::default() }, Deal { id: "2".into(), title: "Control".into(), cut: 90, ..Default::default() }];
        assert_eq!(fresh_big_deals(&deals, &HashSet::new(), &hidden, 75).iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["2"]);
    }

    #[test]
    fn new_free_id_notifies_once_then_is_recorded() {
        let games = vec![game("steam-ftk-1", "Hades", "steam", "free")];
        let mut seen = HashSet::new();
        let fresh = fresh_free(&games, &seen, &[]);
        let n = free_notice(&fresh, false).unwrap();
        assert_eq!(n.title, "Free: Hades");
        assert_eq!(n.body, "Hades is free on Steam!");
        seen.extend(fresh.iter().map(|g| g.id.clone()));
        assert!(fresh_free(&games, &seen, &[]).is_empty());
        assert!(free_notice(&fresh_free(&games, &seen, &[]), false).is_none());
    }

    #[test]
    fn already_seen_free_ids_do_not_notify() {
        let games = vec![
            game("steam-10", "Old", "steam", "free"),
            game("epic-2", "New", "epic", "free"),
        ];
        let seen = HashSet::<String>::from(["steam-10".into()]);
        let fresh = fresh_free(&games, &seen, &[]);
        assert_eq!(fresh.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["epic-2"]);
        let seen = HashSet::<String>::from(["steam-10".into(), "epic-2".into()]);
        assert!(fresh_free(&games, &seen, &[]).is_empty());
    }

    #[test]
    fn ftk_ids_group_on_the_upgrade_poll() {
        // primed is still false, but seen ids from before the steam-ftk- prefix exist.
        assert!(matches!(should_prime(false, false, false), Prime::Notify { mark: true }));
        let games = vec![
            game("steam-10", "Old", "steam", "free"),
            game("steam-ftk-1", "Hades", "steam", "free"),
            game("steam-ftk-2", "Control", "steam", "free"),
        ];
        let mut seen = HashSet::<String>::from(["steam-10".into(), "epic-1".into()]);
        let fresh = fresh_free(&games, &seen, &[]);
        assert_eq!(fresh.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["steam-ftk-1", "steam-ftk-2"]);
        let n = free_notice(&fresh, false).unwrap();
        assert_eq!(n.title, "2 free games available");
        assert_eq!(n.body, "Hades, Control");
        seen.extend(fresh.iter().map(|g| g.id.clone()));
        assert!(fresh_free(&games, &seen, &[]).is_empty());
    }

    #[test]
    fn weekend_and_free_notices_use_different_wording() {
        let seen = HashSet::new();
        let free = free_notice(&fresh_free(&[game("steam-1", "Hades", "steam", "free")], &seen, &[]), false).unwrap();
        assert_eq!(free.title, "Free: Hades");
        assert_eq!(free.body, "Hades is free on Steam!");
        let weekends = vec![
            game("steam-fw-1", "Hades", "steam", "weekend"),
            game("steam-fw-2", "Control", "steam", "weekend"),
        ];
        let grouped = free_notice(&fresh_free(&weekends, &seen, &[]), true).unwrap();
        assert_eq!(grouped.title, "2 free weekend games available");
        assert_eq!(grouped.body, "Hades, Control");
        let one = free_notice(&fresh_free(&weekends[..1], &seen, &[]), true).unwrap();
        assert_eq!(one.title, "Free weekend: Hades");
        assert_eq!(one.body, "Hades is free on Steam!");
    }

    #[test]
    fn wishlist_threshold_and_grouping() {
        let items = vec![
            wish("1", "Nope", Some(19)),
            wish("2", "Barely", None),
            wish("3", "Hades", Some(20)),
        ];
        let fresh = fresh_wishlist(&items, &HashSet::new(), &[], 20);
        assert_eq!(fresh.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["3"]);
        let n = wishlist_notice(&fresh).unwrap();
        assert_eq!(n.title, "🏷️ Hades is on sale!");
        assert_eq!(n.body, "20% off — now $4.99 on Steam");
        assert!(fresh_wishlist(&items, &HashSet::<String>::from(["steam:3".into()]), &[], 20).is_empty());

        let pair = vec![wish("3", "Hades", Some(20)), wish("4", "Control", Some(50))];
        let n = wishlist_notice(&fresh_wishlist(&pair, &HashSet::new(), &[], 20)).unwrap();
        assert_eq!(n.title, "2 wishlist games on sale");
        assert_eq!(n.body, "Hades, Control");

        let many: Vec<_> = (0..5).map(|i| wish(&i.to_string(), &format!("G{i}"), Some(40))).collect();
        let n = wishlist_notice(&fresh_wishlist(&many, &HashSet::new(), &[], 20)).unwrap();
        assert_eq!(n.title, "5 wishlist games on sale");
        assert_eq!(n.body, "G0, G1, G2, G3…");
    }

    #[test]
    fn deeper_wishlist_cut_does_not_renotify() {
        let mut seen = HashSet::new();
        let mut items = vec![wish("10", "Hades", Some(25))];
        let fresh = fresh_wishlist(&items, &seen, &[], 20);
        assert_eq!(fresh.len(), 1);
        remember_wishlist(&mut seen, &items, &fresh);
        items[0].price_info.as_mut().unwrap().discount = 60;
        assert!(fresh_wishlist(&items, &seen, &[], 20).is_empty());
        // Under 5% the id is forgotten, so a later sale notifies again.
        items[0].price_info.as_mut().unwrap().discount = 4;
        remember_wishlist(&mut seen, &items, &[]);
        assert!(!seen.contains("steam:10"));
        items[0].price_info.as_mut().unwrap().discount = 30;
        assert_eq!(fresh_wishlist(&items, &seen, &[], 20).len(), 1);
    }

    #[test]
    fn configurable_notification_cuts_and_hysteresis() {
        assert_eq!(clamp_cut(40, &[10, 20, 30, 50, 75]), 30);
        assert_eq!(clamp_cut(42, &[10, 20, 30, 50, 75]), 50);
        assert_eq!(clamp_cut(50, &[50, 60, 75, 85, 90]), 50);
        let items = vec![wish("1", "A", Some(29)), wish("2", "B", Some(30))];
        assert_eq!(fresh_wishlist(&items, &HashSet::new(), &[], 30).len(), 1);
        let deals = vec![Deal { id: "a".into(), cut: 60, ..Default::default() }, Deal { id: "b".into(), cut: 59, ..Default::default() }];
        assert_eq!(fresh_big_deals(&deals, &HashSet::new(), &[], 75).len(), 0);
        let mut seen = HashSet::from(["a".into(), "b".into()]);
        remember_big_deals(&mut seen, &deals, &[], 75);
        assert!(seen.contains("a"));
        assert!(!seen.contains("b"));
        assert_eq!(fresh_big_deals(&deals, &HashSet::new(), &[], 60).len(), 1);
    }

    #[test]
    fn notification_cut_defaults_are_serialized_in_camel_case() {
        let settings = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(settings["wishlistMinCut"], 20);
        assert_eq!(settings["bigDealMinCut"], 75);
    }

    #[test]
    fn same_title_under_two_ids_notifies_once() {
        let rows = dedupe_free(vec![
            game("steam-ftk-1091500", "Hades", "steam", "free"),
            game("gp-3100", "Hades!", "gamerpower", "free"),
        ]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "steam-ftk-1091500");
        let n = free_notice(&fresh_free(&rows, &HashSet::new(), &[]), false).unwrap();
        assert_eq!(n.title, "Free: Hades");
        assert_eq!(n.body, "Hades is free on Steam!");
    }
}
