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
    notify_sale_soon: bool,
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
    /// Free DLC & loot section on the Free page (opt-in: one extra GamerPower request per poll).
    show_loot: bool,
    /// The first-run tour has been completed or skipped.
    onboarded: bool,
    hidden: Vec<String>,
    /// Wishlist price targets in USD, keyed like `sale_key` ("steam:<id>").
    price_targets: HashMap<String, f64>,
}
impl Default for Settings {
    fn default() -> Self {
        Self { notify_major_sale: true, notify_sale_soon: true, notify_free_game: true, notify_free_weekend: true,
               notify_wishlist_sale: true, wishlist_min_cut: 20, notify_big_deal: false, big_deal_min_cut: 75, check_interval: 60,
               theme: "midnight".into(), base_theme: "midnight".into(), last_sale_theme: String::new(),
               rounded: false, effects: true, launcher_gog: "auto".into(), launcher_epic: "auto".into(), show_loot: false, onboarded: false, hidden: Vec::new(), price_targets: HashMap::new() }
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
    /// Start date ("2026-12-17") of the sale we already sent the day-before heads-up for.
    last_sale_soon_key: Option<String>,
    seen_free_ids: HashSet<String>,
    seen_sale_ids: HashSet<String>,
    seen_deal_ids: HashSet<String>,
    /// Price targets already notified: key → the target that fired (a changed target fires again).
    fired_targets: HashMap<String, f64>,
    /// The first poll has recorded the free list (absent in old config files, so false).
    primed: bool,
}

// ─── App state ────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    free_games: Vec<Free>,
    free_weekends: Vec<Free>,
    /// Epic's next giveaways: shown on the Free page, never notified or counted.
    free_upcoming: Vec<Free>,
    /// GamerPower DLC & in-game items (only when `show_loot`): shown, never notified or counted.
    free_loot: Vec<Free>,
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
            if o.open(&tmp).and_then(|mut f| { std::io::Write::write_all(&mut f, text.as_bytes())?; f.sync_all() }).is_ok() {
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

/// Free games + free weekends that aren't hidden, and wishlist games at least 20% off.
/// Matches the sidebar badges except claimed games (the UI keeps those, the tray can't see them).
/// Hidden games stay in the wishlist count because the Wishlist page and its badge still list them.
/// Keep tray counts and sidebar badges fixed at 20%; only notifications use the preference.
fn tray_counts(s: &Snapshot, hidden: &[String]) -> (usize, usize) {
    let free = s.free_games.iter().chain(&s.free_weekends).filter(|g| !is_hidden(hidden, &g.title)).count();
    (free, s.wishlist.iter().filter(|w| discount(w) >= 20).count())
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
    let hidden = app.state::<AppState>().read(|d| d.settings.hidden.clone());
    let (free, sale) = tray_counts(snap, &hidden);
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

// ─── Sale calendar ────────────────────────────────────────────────────────────

/// Steam's major sales; mirrors `SALES` in renderer/app.js (a test fails if they drift).
struct Sale { name: &'static str, start: &'static str, end: &'static str, est: bool }

const SALES: &[Sale] = &[
    Sale { name: "Spring Sale", start: "2026-03-19", end: "2026-03-26", est: false },
    Sale { name: "Summer Sale", start: "2026-06-25", end: "2026-07-09", est: false },
    Sale { name: "Autumn Sale", start: "2026-10-01", end: "2026-10-08", est: false },
    Sale { name: "Winter Sale", start: "2026-12-17", end: "2027-01-04", est: false },
    Sale { name: "Spring Sale", start: "2027-03-18", end: "2027-03-25", est: false },
    Sale { name: "Summer Sale", start: "2027-06-24", end: "2027-07-08", est: false },
    Sale { name: "Autumn Sale", start: "2027-10-07", end: "2027-10-14", est: true },
];

/// "2026-12-17" → (2026, 12, 17).
fn ymd(s: &str) -> (i64, u32, u32) {
    let mut p = s.split('-').map(|n| n.parse::<i64>().unwrap_or(0));
    let (y, m, d) = (p.next().unwrap_or(0), p.next().unwrap_or(0), p.next().unwrap_or(0));
    (y, m as u32, d as u32)
}

/// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
fn days_from_civil((y, m, d): (i64, u32, u32)) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m as i64 + 9) % 12) + 2) / 5 + d as i64 - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = ((doy - (153 * mp + 2) / 5 + 1) as u32, if mp < 10 { mp + 3 } else { mp - 9 } as u32);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The sale whose first day is the day after `today`.
fn sale_starting_tomorrow(today: (i64, u32, u32)) -> Option<&'static Sale> {
    let tomorrow = days_from_civil(today) + 1;
    SALES.iter().find(|s| days_from_civil(ymd(s.start)) == tomorrow)
}

/// ("Steam Winter Sale starts tomorrow", "Dec 17 – Jan 4. Time to check your wishlist.")
fn sale_soon_notice(sale: &Sale) -> (String, String) {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let day = |s| { let (_, m, d) = ymd(s); format!("{} {d}", MONTHS[(m.clamp(1, 12) - 1) as usize]) };
    let dates = format!("{} – {}", day(sale.start), day(sale.end));
    let body = if sale.est { format!("Expected to start tomorrow ({dates}, dates not announced yet). Time to check your wishlist.") }
               else { format!("{dates}. Time to check your wishlist.") };
    (format!("Steam {} starts tomorrow", sale.name), body)
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

/// "Wishlist prices: <reason>", or None when hot deals already report the same CheapShark cooldown
/// (the "N min" can differ between the two calls, so match the cooldown, not the text).
fn price_failure(err: &str, deals_error: Option<&str>) -> Option<String> {
    let cooldown = |e: &str| e.starts_with(CS_BLOCKED);
    (!(cooldown(err) && deals_error.is_some_and(cooldown))).then(|| failure("Wishlist prices", err))
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

/// CheapShark was unavailable: keep each item's last cheaper offer (if still cheaper) and lowest-ever price.
fn keep_prices(items: &mut [WishItem], prev: &[WishItem]) {
    for w in items {
        let Some(p) = prev.iter().find(|p| p.store == w.store && p.id == w.id) else { continue };
        let fin = w.price_info.as_ref().map(|x| x.fin);
        w.cheapest = p.cheapest.clone().filter(|c| fin.is_some_and(|fin| c.price + 0.005 < fin));
        w.lowest = p.lowest;
    }
}

/// CheapShark gave no answer for these items this poll (a failed request, or past the lookup cap):
/// keep the last cheaper offer (and lowest-ever price) while the store's own price is unchanged, so one bad poll neither
/// hides it nor re-arms its price target (which would notify again on the next good poll).
/// ponytail: an item that stays past the cap keeps an offer that may have ended, until its own price
/// changes; rotate the lookups if that shows up.
fn carry_cheapest(items: &mut [WishItem], prev: &[WishItem], unanswered: &[usize]) {
    let own = |w: &WishItem| w.price_info.as_ref().map(|p| (p.fin, p.currency.clone()));
    for &i in unanswered {
        let w = &mut items[i];
        let Some(p) = prev.iter().find(|p| p.store == w.store && p.id == w.id) else { continue };
        if own(p) == own(w) { w.cheapest = p.cheapest.clone(); w.lowest = w.lowest.or(p.lowest); }
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
        // app.js syncSaleTheme matches this "Steam sale detection" name to keep the theme as is
        Err(e) => { eprintln!("[sale-detect] {e}"); source_errors.lock().unwrap().push(failure("Steam sale detection", &e)); prev.live_sale.clone() }
    };

    // 1b. Heads-up the day before a major sale (from the SALES calendar, once per sale).
    // ponytail: UTC date, not the user's local one (std has no time zones), so it can fire a few hours early/late.
    if settings.notify_sale_soon {
        if let Some(sale) = sale_starting_tomorrow(civil_from_days((now_ms() / 86_400_000) as i64)) {
            if st.read(|d| d.last_sale_soon_key.as_deref() != Some(sale.start)) {
                let (title, body) = sale_soon_notice(sale);
                notify(app, &title, &body);
                st.edit(|d| d.last_sale_soon_key = Some(sale.start.into()));
            }
        }
    }

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
    let loot = async { if settings.show_loot { Some(gamerpower_loot().await) } else { None } };
    let (ef, sf, sk, sw, gp, lt, ss, gs, es, hot) = tokio::join!(
        epic_free(), steam_free_featured(), steam_free_to_keep(), steam_free_weekends(), gamerpower(), loot,
        steam_sales(), gog_sales(), stores::epic_sales(), hot_deals(),
    );
    // Not part of the first-poll seed (never notified), so a failure doesn't set free_failed.
    let free_loot = match lt {
        None => vec![],
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            eprintln!("[GamerPower loot] {e}"); source_errors.lock().unwrap().push(failure("GamerPower loot", &e));
            restore_by_prefix(&prev.free_loot, "gp-loot-")
        }
    };
    // One Epic response feeds both lists; a failure keeps the last good upcoming list too.
    let (ef, free_upcoming) = match ef { Ok((now, next)) => (Ok(now), next), Err(e) => (Err(e), prev.free_upcoming.clone()) };
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
    let (mut deals, deals_source, deals_error) = hot;
    if deals_error.is_some() { deals = prev.deals.clone(); } // keep the last good hot deals

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

    match attach_cheapest(&mut wishlist).await {
        Ok(unanswered) => carry_cheapest(&mut wishlist, &prev.wishlist, &unanswered),
        Err(e) => {
            eprintln!("[wishlist prices] {e}");
            source_errors.lock().unwrap().extend(price_failure(&e, deals_error.as_deref()));
            keep_prices(&mut wishlist, &prev.wishlist);
        }
    }
    if settings.notify_wishlist_sale { notify_wishlist(app, st, &wishlist); }
    notify_targets(app, st, &wishlist); // setting a target is the opt-in, so no extra toggle
    if settings.notify_big_deal { notify_big_deals(app, st, &deals); }

    Snapshot {
        free_games, free_weekends, free_upcoming, free_loot, sales, live_sale, deals, deals_source, deals_error,
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

/// Record every listed game, hidden ones too, so unhiding one later does not announce it as new.
fn remember_free(seen: &mut HashSet<String>, games: &[Free]) {
    seen.extend(games.iter().map(|g| g.id.clone()));
}

fn notify_new_free(app: &AppHandle, st: &AppState, games: &[Free], weekend: bool) {
    let (fresh, unseen) = st.read(|d| (fresh_free(games, &d.seen_free_ids, &d.settings.hidden), games.iter().any(|g| !d.seen_free_ids.contains(&g.id))));
    if !unseen { return; }
    st.edit(|d| remember_free(&mut d.seen_free_ids, games));
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

/// Record every sale at the cut (what we just notified, plus hidden games so unhiding one does not
/// announce an old sale). Forget a game once its discount falls under 5% so the next
/// sale notifies again. A deeper cut while it stays at 5% or more does not.
fn remember_wishlist(seen: &mut HashSet<String>, items: &[WishItem], min_cut: u32) {
    seen.extend(items.iter().filter(|w| discount(w) >= min_cut).map(sale_key));
    for w in items { if discount(w) < 5 { seen.remove(&sale_key(w)); } }
}

fn notify_wishlist(app: &AppHandle, st: &AppState, items: &[WishItem]) {
    let (fresh, min_cut) = st.read(|d| (fresh_wishlist(items, &d.seen_sale_ids, &d.settings.hidden, d.settings.wishlist_min_cut), d.settings.wishlist_min_cut));
    if let Some(n) = wishlist_notice(&fresh) { notify(app, &n.title, &n.body); }
    st.edit(|d| remember_wishlist(&mut d.seen_sale_ids, items, min_cut));
}

/// Price compared with a target: the cheaper offer elsewhere if any (the link the UI opens), else the store's own.
/// USD only: targets are entered in dollars and CheapShark prices are USD.
fn target_price(w: &WishItem) -> Option<f64> {
    let p = w.price_info.as_ref().filter(|p| p.currency.as_deref() == Some("USD"))?;
    Some(w.cheapest.as_ref().map_or(p.fin, |c| c.price.min(p.fin)))
}

/// A wishlist item with its price and the target it reached.
type TargetHit<'a> = (&'a WishItem, f64, f64);

/// Items at or under their target that have not fired for that target yet (with price and target),
/// and the new fired map: a target re-arms once the price goes back above it or the target changes.
/// Hidden games never notify; an item missing from this poll (or without a USD price) keeps its state.
fn target_hits<'a>(items: &'a [WishItem], targets: &HashMap<String, f64>, fired: &HashMap<String, f64>, hidden: &[String])
    -> (Vec<TargetHit<'a>>, HashMap<String, f64>) {
    let mut next: HashMap<String, f64> = fired.iter().filter(|(k, v)| targets.get(*k) == Some(v)).map(|(k, v)| (k.clone(), *v)).collect();
    let mut hits = vec![];
    for w in items {
        let key = sale_key(w);
        let (Some(&target), Some(price)) = (targets.get(&key), target_price(w)) else { continue };
        if price > target + 0.005 { next.remove(&key); }
        else if !next.contains_key(&key) && !is_hidden(hidden, &w.name) { next.insert(key, target); hits.push((w, price, target)); }
    }
    (hits, next)
}

fn target_notice(hits: &[TargetHit]) -> Option<Notice> {
    match hits {
        [] => None,
        [(w, price, target)] => {
            let store = w.cheapest.as_ref().map_or(store_label(&w.store), |c| c.store.as_str()); // set only when cheaper
            Some(Notice { title: format!("Under your target: {}", w.name), body: format!("{} is ${price:.2} on {store} (target ${target:.2})", w.name) })
        }
        many => {
            let names: Vec<&str> = many.iter().take(4).map(|h| h.0.name.as_str()).collect();
            Some(Notice { title: format!("{} wishlist games under your target", many.len()), body: names.join(", ") + if many.len() > 4 { "…" } else { "" } })
        }
    }
}

fn notify_targets(app: &AppHandle, st: &AppState, items: &[WishItem]) {
    let (hits, fired) = st.read(|d| target_hits(items, &d.settings.price_targets, &d.fired_targets, &d.settings.hidden));
    if let Some(n) = target_notice(&hits) { notify(app, &n.title, &n.body); }
    st.edit(|d| d.fired_targets = fired);
}

/// Keep targets from $0.01 to $10,000 (rounded to cents) on "store:id" keys, at most 500.
fn normalize_targets(targets: HashMap<String, f64>) -> HashMap<String, f64> {
    let mut ok: Vec<(String, f64)> = targets.into_iter()
        .map(|(k, v)| (k, (v * 100.0).round() / 100.0))
        .filter(|(k, v)| v.is_finite() && *v >= 0.01 && *v <= 10_000.0 && k.len() <= 200 && k.contains(':'))
        .collect();
    ok.sort_by(|a, b| a.0.cmp(&b.0)); // ponytail: the cap drops by key order, not age; nobody sets 500 targets
    ok.truncate(500);
    ok.into_iter().collect()
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
        notify(app, &format!("{} deals at {}%+ off", fresh.len(), min_cut), &(names.join(", ") + if fresh.len() > 4 { "…" } else { "" }));
    }
    // Hysteresis stays 15 points below the selected notification threshold.
    st.edit(|s| remember_big_deals(&mut s.seen_deal_ids, deals, min_cut));
}

/// Every deal at the cut is recorded, hidden ones too, so unhiding one does not announce it as new.
fn remember_big_deals(seen: &mut HashSet<String>, deals: &[Deal], min_cut: u32) {
    seen.extend(deals.iter().filter(|d| d.cut >= min_cut).map(|d| d.id.clone()));
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
fn save_settings(app: AppHandle, st: State<AppState>, settings: Settings) {
    let mut settings = settings;
    settings.hidden = normalize_hidden(settings.hidden);
    settings.price_targets = normalize_targets(settings.price_targets);
    settings.wishlist_min_cut = clamp_cut(settings.wishlist_min_cut, &[10, 20, 30, 50, 75]);
    settings.big_deal_min_cut = clamp_cut(settings.big_deal_min_cut, &[50, 60, 75, 85, 90]);
    // A new interval, or loot just switched on (fetch it now rather than at the next poll)
    let changed = st.edit(|d| {
        let c = d.settings.check_interval != settings.check_interval || (settings.show_loot && !d.settings.show_loot);
        d.settings = settings; c
    });
    if changed { st.wake.notify_one(); }
    // hiding a game changes the tray counts now, not at the next poll
    let snap = st.snapshot.lock().unwrap().clone();
    if let Some(snap) = snap { update_tray(&app, &snap); }
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
    let why = match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Db::default(),
        Err(e) => e.to_string(),
        Ok(bytes) => match serde_json::from_slice(&bytes) { Ok(db) => return db, Err(e) => e.to_string() },
    };
    // Keep the unreadable file (sign-in tokens, settings) instead of overwriting it on the next save,
    // and never replace an earlier one: config.bad.json, config.bad2.json, …
    let bad = (1..).map(|n| path.with_extension(if n == 1 { "bad.json".into() } else { format!("bad{n}.json") }))
        .find(|p| !p.exists()).unwrap();
    match std::fs::rename(path, &bad) {
        Ok(()) => eprintln!("config.json is unreadable ({why}); moved it to {}", bad.display()),
        Err(e) => eprintln!("config.json is unreadable ({why}) and couldn't be moved to {} ({e}); fix or remove it", bad.display()),
    }
    Db::default()
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
            std::thread::spawn(autostart::refresh); // off the startup path: Windows runs `reg`

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
    use super::{civil_from_days, days_from_civil, sale_soon_notice, sale_starting_tomorrow, SALES, clamp_cut, dedupe_free, failure, free_notice, fresh_big_deals, fresh_free, fresh_wishlist, is_hidden, keep_prices, carry_cheapest, normalize_hidden, normalize_targets, target_hits, target_notice, price_failure, CS_BLOCKED, normalize_title, remember_big_deals, remember_free, remember_wishlist, restore_by_prefix, safe_https, should_prime, store_label, tray_counts, tray_tooltip, with_count, wishlist_notice, load_db, Acct, Deal, Free, Price, Prime, Settings, WishItem};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn unreadable_config_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("dealdrop-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        let db = load_db(&path);
        assert!(db.steam_id.is_none() && !db.primed);
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(dir.join("config.bad.json")).unwrap(), "{ not json");
        assert!(load_db(&path).steam_id.is_none()); // missing file: defaults, nothing moved
        std::fs::write(&path, b"{\"steamId\":\"\xff\"}").unwrap(); // not UTF-8: also kept, next to the first
        load_db(&path);
        assert!(!path.exists() && dir.join("config.bad2.json").exists());
        assert_eq!(std::fs::read_to_string(dir.join("config.bad.json")).unwrap(), "{ not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sale_calendar_matches_the_ui() {
        let js = include_str!("../../renderer/app.js");
        let block = &js[js.find("const SALES = [").unwrap()..];
        let block = &block[..block.find("\n]").unwrap()];
        for s in SALES {
            let row = format!("{{ name: '{}', start: '{}', end: '{}'", s.name, s.start, s.end);
            let line = block.lines().find(|l| l.contains(&row)).unwrap_or_else(|| panic!("{row} missing from app.js SALES"));
            assert_eq!(line.contains("est: true"), s.est, "{row}: est differs");
        }
        assert_eq!(block.matches("{ name:").count(), SALES.len(), "app.js SALES has rows the Rust table lacks");
    }

    #[test]
    fn sale_heads_up_only_the_day_before() {
        assert_eq!(sale_starting_tomorrow((2026, 12, 16)).map(|s| s.start), Some("2026-12-17"));
        assert!(sale_starting_tomorrow((2026, 12, 17)).is_none()); // already started
        assert!(sale_starting_tomorrow((2026, 12, 15)).is_none());
        assert!(sale_starting_tomorrow((2026, 10, 18)).is_none());
        assert_eq!(sale_starting_tomorrow((2026, 9, 30)).map(|s| s.name), Some("Autumn Sale")); // month boundary
        assert_eq!(days_from_civil((2026, 12, 31)) + 1, days_from_civil((2027, 1, 1))); // year boundary
        for d in [-1, 0, 20_000, days_from_civil((2026, 12, 31)), days_from_civil((2028, 2, 29))] {
            assert_eq!(days_from_civil(civil_from_days(d)), d);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let winter = sale_starting_tomorrow((2026, 12, 16)).unwrap();
        assert_eq!(sale_soon_notice(winter), ("Steam Winter Sale starts tomorrow".into(), "Dec 17 – Jan 4. Time to check your wishlist.".into()));
        assert!(sale_soon_notice(SALES.last().unwrap()).1.starts_with("Expected to start tomorrow (Oct 7 – Oct 14"));
    }

    #[test]
    fn tray_text_counts_unhidden_free_games_and_wishlist_sales() {
        assert_eq!(tray_tooltip(0, 0), "DealDrop");
        assert_eq!(tray_tooltip(3, 5), "DealDrop: 3 free games · 5 wishlist games on sale");
        assert_eq!(tray_tooltip(1, 0), "DealDrop: 1 free game");
        assert_eq!(tray_tooltip(0, 1), "DealDrop: 1 wishlist game on sale");
        assert_eq!(with_count("Free now", 3), "Free now (3)");
        assert_eq!(with_count("Wishlist", 0), "Wishlist");
        let wish = |discount| WishItem { price_info: Some(Price { discount, ..Default::default() }), ..Default::default() };
        let snap = super::Snapshot {
            free_games: vec![Free::default(), Free::default(), Free { title: "Hades  II".into(), ..Default::default() }],
            free_weekends: vec![Free::default(), Free { title: "hades ii".into(), ..Default::default() }],
            wishlist: vec![wish(20), wish(19), WishItem::default(), WishItem { name: "Hades II".into(), ..wish(75) }],
            ..Default::default()
        };
        assert_eq!(tray_counts(&snap, &[]), (5, 2));
        // hidden free games drop out; the Wishlist page still lists hidden games, so they still count there
        assert_eq!(tray_counts(&snap, &["Hades II".into()]), (3, 2));
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
    fn cheapshark_cooldown_is_reported_once() {
        let cd = |m| format!("{CS_BLOCKED}; trying again in {m} min (14:01 UTC)");
        assert_eq!(price_failure(&cd(14), Some(&cd(15))), None);
        assert_eq!(price_failure(&cd(14), None).unwrap(), format!("Wishlist prices: {}", cd(14)));
        assert!(price_failure(&cd(14), Some("HTTP 500")).is_some());
        assert!(price_failure("HTTP 500", Some(&cd(14))).is_some());
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
    fn keep_prices_restores_last_good_cheapshark_data() {
        let cheap = |price| Some(super::Cheapest { store: "GMG".into(), price, url: String::new() });
        let mut old = vec![wish("1", "A", Some(50)), wish("2", "B", Some(50)), wish("3", "C", Some(0))];
        old[0].cheapest = cheap(3.0); old[0].lowest = Some(2.5);
        old[1].cheapest = cheap(3.0);
        let mut now = vec![wish("1", "A", Some(50)), wish("2", "B", Some(50)), wish("9", "New", Some(10))];
        now[0].price_info.as_mut().unwrap().fin = 5.0;
        now[1].price_info.as_mut().unwrap().fin = 2.0; // now cheaper on its own store than the old offer
        keep_prices(&mut now, &old);
        assert_eq!(now[0].cheapest.as_ref().map(|c| c.price), Some(3.0));
        assert_eq!(now[0].lowest, Some(2.5));
        assert!(now[1].cheapest.is_none());
        assert!(now[2].cheapest.is_none() && now[2].lowest.is_none());
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
    fn hidden_matches_are_recorded_so_unhiding_does_not_notify() {
        let hidden = vec!["Hades".to_string()];
        let free = vec![game("steam-1", "Hades", "steam", "free")];
        let mut seen = HashSet::new();
        assert!(fresh_free(&free, &seen, &hidden).is_empty());
        remember_free(&mut seen, &free);
        assert!(fresh_free(&free, &seen, &[]).is_empty());

        let wishlist = vec![wish("1", "Hades", Some(40)), wish("2", "Control", Some(10))];
        let mut seen = HashSet::new();
        assert!(fresh_wishlist(&wishlist, &seen, &hidden, 20).is_empty());
        remember_wishlist(&mut seen, &wishlist, 20);
        assert!(fresh_wishlist(&wishlist, &seen, &[], 20).is_empty());
        assert!(!seen.contains("steam:2")); // under the cut: a later deeper sale still notifies

        let deals = vec![Deal { id: "1".into(), title: "Hades".into(), cut: 90, ..Default::default() }];
        let mut seen = HashSet::new();
        assert!(fresh_big_deals(&deals, &seen, &hidden, 75).is_empty());
        remember_big_deals(&mut seen, &deals, 75);
        assert!(fresh_big_deals(&deals, &seen, &[], 75).is_empty());
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
        remember_wishlist(&mut seen, &items, 20);
        items[0].price_info.as_mut().unwrap().discount = 60;
        assert!(fresh_wishlist(&items, &seen, &[], 20).is_empty());
        // Under 5% the id is forgotten, so a later sale notifies again.
        items[0].price_info.as_mut().unwrap().discount = 4;
        remember_wishlist(&mut seen, &items, 20);
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
        remember_big_deals(&mut seen, &deals, 75);
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

    #[test]
    fn price_targets_fire_once_and_re_arm() {
        use super::Cheapest;
        let item = |id: &str, fin: f64, cur: &str| WishItem { store: "steam".into(), id: id.into(), name: format!("Game {id}"),
            price_info: Some(Price { fin, currency: Some(cur.into()), ..Default::default() }), ..Default::default() };
        let targets = HashMap::from([("steam:a".to_string(), 10.0)]);
        let none = HashMap::new();
        // ids notified and the new fired map
        let run = |items: Vec<WishItem>, targets: &HashMap<String, f64>, fired: &HashMap<String, f64>, hidden: &[String]| {
            let (hits, next) = target_hits(&items, targets, fired, hidden);
            (hits.iter().map(|h| h.0.id.clone()).collect::<Vec<_>>(), next)
        };

        // hit (at the target counts), then already fired
        let (hits, fired) = run(vec![item("a", 10.0, "USD")], &targets, &none, &[]);
        assert_eq!(hits, ["a"]);
        assert_eq!(fired, HashMap::from([("steam:a".to_string(), 10.0)]));
        let (hits, still) = run(vec![item("a", 8.0, "USD")], &targets, &fired, &[]);
        assert!(hits.is_empty() && still == fired);
        // no hit above the target, which also re-arms it
        let (hits, rearmed) = run(vec![item("a", 12.0, "USD")], &targets, &fired, &[]);
        assert!(hits.is_empty() && rearmed.is_empty());
        assert_eq!(run(vec![item("a", 9.0, "USD")], &targets, &rearmed, &[]).0, ["a"]);
        // a missing item keeps its fired state
        assert_eq!(run(vec![], &targets, &fired, &[]).1, fired);
        // a changed target fires again; a cleared one is forgotten
        let lower = HashMap::from([("steam:a".to_string(), 9.0)]);
        assert_eq!(run(vec![item("a", 8.0, "USD")], &lower, &fired, &[]).0, ["a"]);
        assert!(run(vec![item("a", 8.0, "USD")], &none, &fired, &[]).1.is_empty());
        // non-USD and hidden games are skipped
        assert!(run(vec![item("a", 5.0, "EUR")], &targets, &none, &[]).0.is_empty());
        let (hits, fired) = run(vec![item("a", 5.0, "USD")], &targets, &none, &["game A".into()]);
        assert!(hits.is_empty() && fired.is_empty());
        // the cheaper offer elsewhere counts, and the notice names that store
        let mut w = item("a", 15.0, "USD");
        w.cheapest = Some(Cheapest { store: "Fanatical".into(), price: 9.5, url: String::new() });
        let (hits, _) = target_hits(std::slice::from_ref(&w), &targets, &none, &[]);
        let n = target_notice(&hits).unwrap();
        assert_eq!((n.title.as_str(), n.body.as_str()), ("Under your target: Game a", "Game a is $9.50 on Fanatical (target $10.00)"));
    }

    #[test]
    fn a_missed_cheapshark_lookup_does_not_rearm_a_price_target() {
        use super::Cheapest;
        let targets = HashMap::from([("steam:a".to_string(), 10.0)]);
        let mut item = wish("a", "Game a", Some(0));
        item.price_info.as_mut().unwrap().fin = 20.0;
        item.price_info.as_mut().unwrap().currency = Some("USD".into());
        let mut offer = item.clone();
        offer.cheapest = Some(Cheapest { store: "Fanatical".into(), price: 9.5, url: String::new() });
        offer.lowest = Some(8.0);
        let (hits, fired) = target_hits(std::slice::from_ref(&offer), &targets, &HashMap::new(), &[]);
        assert_eq!(hits.len(), 1);
        // next poll CheapShark has no answer for it: the offer carries over and the target stays fired
        let mut now = vec![item.clone()];
        carry_cheapest(&mut now, std::slice::from_ref(&offer), &[0]);
        assert_eq!(now[0].lowest, Some(8.0)); // its "Lowest ever" tag doesn't flicker off either
        let (hits, fired) = target_hits(&now, &targets, &fired, &[]);
        assert!(hits.is_empty() && fired.contains_key("steam:a"));
        // a changed store price drops the old offer; an answered item is left alone
        let mut now = vec![item.clone(), item];
        now[0].price_info.as_mut().unwrap().fin = 19.0;
        carry_cheapest(&mut now, std::slice::from_ref(&offer), &[0]);
        assert!(now[0].cheapest.is_none() && now[1].cheapest.is_none() && now[0].lowest.is_none());
    }

    #[test]
    fn price_targets_are_validated_on_save() {
        let raw = HashMap::from([
            ("steam:a".to_string(), 9.999), ("steam:b".into(), 0.0), ("steam:c".into(), -1.0), ("steam:d".into(), f64::NAN),
            ("steam:e".into(), f64::INFINITY), ("steam:f".into(), 10_000.01), ("nokey".into(), 5.0), ("gog:g".into(), 10_000.0),
        ]);
        assert_eq!(normalize_targets(raw), HashMap::from([("steam:a".to_string(), 10.0), ("gog:g".into(), 10_000.0)]));
        assert_eq!(normalize_targets((0..600).map(|i| (format!("steam:{i}"), 1.0)).collect()).len(), 500);
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["priceTargets"], serde_json::json!({}));
    }
}
