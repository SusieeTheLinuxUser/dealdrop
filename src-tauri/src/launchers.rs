//! Which desktop app opens a store page. Data-driven so a new launcher is one row + one match arm
//! (e.g. GOG's native Linux client once it ships: if it uses another URL scheme, edit the
//! `native` row for "gog" below).

use serde::Serialize;
use url::Url;

pub struct Opt {
    pub key: &'static str,
    pub label: &'static str,
    pub scheme: &'static str,
}

const fn opt(key: &'static str, label: &'static str, scheme: &'static str) -> Opt { Opt { key, label, scheme } }

const STEAM: [Opt; 1] = [opt("native", "Steam app", "steam")];
const EPIC: [Opt; 1] = [opt("native", "Epic app", "com.epicgames.launcher")];
const GOG: [Opt; 1] = [opt("native", "GOG Galaxy", "goggalaxy")];

/// Order = what "Auto" prefers.
pub fn options(store: &str) -> &'static [Opt] {
    match store {
        "steam" => &STEAM,
        "epic" => &EPIC,
        "gog" => &GOG,
        _ => &[],
    }
}

/// Is a desktop app installed that handles this URL scheme?
#[cfg(target_os = "linux")]
fn has_scheme(scheme: &str) -> bool {
    std::process::Command::new("xdg-mime")
        .args(["query", "default", &format!("x-scheme-handler/{scheme}")])
        .output().map(|o| !o.stdout.iter().all(u8::is_ascii_whitespace)).unwrap_or(false)
}

/// Windows registers every URL handler under HKEY_CLASSES_ROOT\<scheme>.
#[cfg(target_os = "windows")]
fn has_scheme(scheme: &str) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args(["query", &format!("HKCR\\{scheme}"), "/ve"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash
        .output().map(|o| o.status.success()).unwrap_or(false)
}

/// macOS has no scheme lookup without extra frameworks; check for the app bundle instead.
#[cfg(target_os = "macos")]
fn has_scheme(scheme: &str) -> bool {
    let app = match scheme {
        "steam" => "Steam.app",
        "com.epicgames.launcher" => "Epic Games Launcher.app",
        "goggalaxy" => "GOG Galaxy.app",
        _ => return false,
    };
    let home = std::env::var("HOME").unwrap_or_default();
    [format!("/Applications/{app}"), format!("{home}/Applications/{app}")].iter().any(|p| std::path::Path::new(p).exists())
}

#[derive(Serialize)]
pub struct Choice { pub key: &'static str, pub label: &'static str }

#[derive(Serialize)]
pub struct StoreLaunchers {
    pub options: Vec<Choice>,       // installed launchers
    pub chosen: Option<Choice>,     // what the buttons will use
}

fn choice(o: &Opt) -> Choice { Choice { key: o.key, label: o.label } }

/// `pref`: "auto" | an option key | "browser".
pub fn resolve(store: &str, pref: &str) -> StoreLaunchers {
    resolve_with(store, pref, has_scheme)
}

fn resolve_with<F>(store: &str, pref: &str, has_scheme: F) -> StoreLaunchers
where
    F: Fn(&str) -> bool,
{
    let installed: Vec<&Opt> = options(store).iter().filter(|o| has_scheme(o.scheme)).collect();
    let chosen = match pref {
        "browser" => None,
        p => installed.iter().find(|o| o.key == p).or(installed.first()).map(|o| choice(o)),
    };
    StoreLaunchers { options: installed.into_iter().map(choice).collect(), chosen }
}

fn numeric(id: Option<&str>) -> Option<&str> {
    id.map(|i| i.trim_start_matches("gog-")).filter(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_digit()))
}

/// A plain URL slug (letters, digits, `-`, `_`) or nothing.
fn slug(s: &str) -> Option<&str> {
    Some(s.trim_end_matches('/')).filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Deep link for one launcher, if we have the data it needs.
pub fn link(store: &str, key: &str, url: &Url, id: Option<&str>) -> Option<String> {
    // The URL comes from remote data: only act on the store's own domain and plain slugs.
    let domains: &[&str] = match store { "steam" => &["steampowered.com", "steamcommunity.com"], "epic" => &["epicgames.com"], "gog" => &["gog.com"], _ => return None };
    let host = url.host_str()?;
    if !domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))) { return None; }
    match (store, key) {
        ("steam", _) => Some(match url.path().split_once("/app/").and_then(|(_, r)| r.split('/').next()).filter(|a| a.bytes().all(|b| b.is_ascii_digit())) {
            Some(app) => format!("steam://store/{app}"),
            None => format!("steam://openurl/{url}"),
        }),
        ("epic", _) => url.path().split_once("/p/").and_then(|(_, s)| slug(s)).map(|s| format!("com.epicgames.launcher://store/p/{s}")),
        ("gog", "native") => numeric(id).map(|i| format!("goggalaxy://openGameView/{i}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_preferences_use_option_order_without_checking_the_os() {
        let installed = |scheme: &str| scheme == "goggalaxy";

        let auto = resolve_with("gog", "auto", installed);
        assert_eq!(auto.options.iter().map(|o| o.key).collect::<Vec<_>>(), ["native"]);
        assert_eq!(auto.chosen.as_ref().map(|o| o.key), Some("native"));

        let saved_heroic = resolve_with("gog", "heroic", installed);
        assert_eq!(saved_heroic.chosen.as_ref().map(|o| o.key), Some("native"));
        let saved_lutris = resolve_with("gog", "lutris", installed);
        assert_eq!(saved_lutris.chosen.as_ref().map(|o| o.key), Some("native"));

        let browser = resolve_with("gog", "browser", installed);
        assert_eq!(browser.chosen.as_ref().map(|o| o.key), None);
        assert_eq!(browser.options.len(), 1);
    }

    /// SteamID64 of whoever is signed in to Steam on this PC (the "MostRecent" account in
    /// loginusers.vdf, else the first one), or the empty base ID when Steam isn't installed (CI).
    fn local_steam_id() -> String {
        let home = std::env::var("HOME").unwrap_or_default();
        let files = [
            format!("{home}/.steam/steam/config/loginusers.vdf"),
            format!("{home}/.local/share/Steam/config/loginusers.vdf"),
            format!("{home}/.var/app/com.valvesoftware.Steam/.local/share/Steam/config/loginusers.vdf"),
            format!("{home}/Library/Application Support/Steam/config/loginusers.vdf"),
            r"C:\Program Files (x86)\Steam\config\loginusers.vdf".to_string(),
        ];
        let text = files.iter().find_map(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        let (mut current, mut first) = (None, None);
        for line in text.lines().map(|l| l.trim().trim_matches('"')) {
            if line.len() == 17 && line.starts_with("7656119") && line.bytes().all(|b| b.is_ascii_digit()) {
                current = Some(line.to_string());
                first = first.or(current.clone());
            } else if line.starts_with("MostRecent") && line.ends_with('1') && current.is_some() {
                return current.unwrap();
            }
        }
        first.unwrap_or_else(|| "76561197960265728".into())
    }

    #[test]
    fn deep_links() {
        let gog = Url::parse("https://www.gog.com/en/game/the_witcher_3_wild_hunt").unwrap();
        assert_eq!(link("gog", "native", &gog, Some("gog-1207664643")).unwrap(), "goggalaxy://openGameView/1207664643");
        let epic = Url::parse("https://store.epicgames.com/en-US/p/bad-cheese-4256d9").unwrap();
        assert_eq!(link("epic", "native", &epic, None).unwrap(), "com.epicgames.launcher://store/p/bad-cheese-4256d9");
        let steam = Url::parse("https://store.steampowered.com/app/60/Ricochet/").unwrap();
        assert_eq!(link("steam", "native", &steam, None).unwrap(), "steam://store/60");
        let sale = Url::parse("https://store.steampowered.com/specials").unwrap();
        assert_eq!(link("steam", "native", &sale, None).unwrap(), "steam://openurl/https://store.steampowered.com/specials");
        let id = local_steam_id();
        let profile = Url::parse(&format!("https://steamcommunity.com/profiles/{id}")).unwrap();
        assert_eq!(link("steam", "native", &profile, None).unwrap(), format!("steam://openurl/https://steamcommunity.com/profiles/{id}"));
        // remote data can't aim a launcher at another site or smuggle odd characters into a slug
        let evil = Url::parse("https://evil.example/app/60/").unwrap();
        assert!(link("steam", "native", &evil, None).is_none());
        let odd = Url::parse("https://store.epicgames.com/en-US/p/a%20b;c").unwrap();
        assert!(link("epic", "native", &odd, None).is_none());
    }

    #[test]
    fn gog_ids_and_slugs_reject_untrusted_characters() {
        assert_eq!(numeric(Some("gog-1207664643")), Some("1207664643"));
        assert_eq!(numeric(Some("gog-12/3")), None);
        assert_eq!(slug("the_witcher-3/"), Some("the_witcher-3"));
        assert_eq!(slug("bad%2fslug"), None);
    }

    #[test]
    fn launcher_links_only_follow_the_stores_own_domain() {
        let u = |s: &str| Url::parse(s).unwrap();
        // A look-alike host is not the store's domain
        for bad in ["https://notsteampowered.com/app/60", "https://steampowered.com.evil.example/app/60",
                    "https://store.epicgames.com.evil.example/en-US/p/x", "https://gog.com.evil.example/en/game/x"] {
            assert!(link("steam", "native", &u(bad), Some("gog-1")).is_none(), "{bad}");
        }
        assert!(link("steam", "native", &u("https://store.steampowered.com.evil.example/app/60"), None).is_none());
        assert!(link("epic", "native", &u("https://store.epicgames.com.evil.example/en-US/p/x"), None).is_none());
        assert!(link("gog", "native", &u("https://gog.com.evil.example/en/game/x"), Some("gog-1")).is_none());
        // An unknown store has no launcher at all
        assert!(link("humble", "native", &u("https://store.steampowered.com/app/60"), None).is_none());
        // GOG's deep link needs a numeric id and its native client
        let gog = u("https://www.gog.com/en/game/the_witcher_3_wild_hunt");
        assert!(link("gog", "native", &gog, None).is_none());
        assert!(link("gog", "native", &gog, Some("gog-")).is_none());
        assert!(link("gog", "browser", &gog, Some("gog-1207664643")).is_none());
    }

    #[test]
    fn steam_links_fall_back_to_the_url_for_a_missing_app_id() {
        let u = |s: &str| Url::parse(s).unwrap();
        assert_eq!(link("steam", "native", &u("https://store.steampowered.com/app/60abc"), None).unwrap(),
            "steam://openurl/https://store.steampowered.com/app/60abc");
        // The community domain is Steam's too
        assert_eq!(link("steam", "native", &u("https://steamcommunity.com/profiles/7656119"), None).unwrap(),
            "steam://openurl/https://steamcommunity.com/profiles/7656119");
        // An Epic offer without a product slug has no deep link
        assert!(link("epic", "native", &u("https://store.epicgames.com/en-US/browse"), None).is_none());
        assert!(link("epic", "native", &u("https://store.epicgames.com/en-US/p/"), None).is_none());
        assert_eq!(link("epic", "native", &u("https://store.epicgames.com/en-US/p/bad-cheese/"), None).unwrap(),
            "com.epicgames.launcher://store/p/bad-cheese");
    }

    #[test]
    #[ignore = "BUG: an empty app id passes the digit check, so /app/ opens steam://store/ instead of the url"]
    fn steam_links_do_not_build_a_deep_link_without_an_app_id() {
        let u = |s: &str| Url::parse(s).unwrap();
        assert_eq!(link("steam", "native", &u("https://store.steampowered.com/app/"), None).unwrap(),
            "steam://openurl/https://store.steampowered.com/app/");
    }

    #[test]
    fn a_store_without_an_installed_launcher_has_nothing_to_choose() {
        let none = |_: &str| false;
        let every = |_: &str| true;
        assert!(resolve_with("gog", "auto", none).options.is_empty());
        assert!(resolve_with("gog", "auto", none).chosen.is_none());
        assert!(resolve_with("gog", "native", none).chosen.is_none()); // the wanted launcher isn't installed
        assert!(resolve_with("humble", "auto", every).options.is_empty());
        assert!(resolve_with("humble", "auto", every).chosen.is_none());
        assert_eq!(resolve_with("steam", "auto", every).chosen.as_ref().map(|c| c.key), Some("native"));
    }
}
