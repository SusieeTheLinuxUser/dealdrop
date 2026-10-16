//! Which desktop app opens a store page. Data-driven so a new launcher is one row + one match arm
//! (e.g. GOG's native Linux client once it ships: if it uses another URL scheme, edit the
//! `native` row for "gog" below — Auto already prefers `native` over Heroic/Lutris).

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
// Heroic only knows games in the user's library (it offers to install them); Lutris resolves the
// store slug on lutris.net. Both are best-effort for games the user doesn't own yet.
const GOG: [Opt; 3] = [opt("native", "GOG Galaxy", "goggalaxy"), opt("heroic", "Heroic", "heroic"), opt("lutris", "Lutris", "lutris")];

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
        "heroic" => "Heroic.app",
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
    let domain = match store { "steam" => "steampowered.com", "epic" => "epicgames.com", "gog" => "gog.com", _ => return None };
    let host = url.host_str()?;
    if host != domain && !host.ends_with(&format!(".{domain}")) { return None; }
    match (store, key) {
        ("steam", _) => Some(match url.path().split_once("/app/").and_then(|(_, r)| r.split('/').next()).filter(|a| a.bytes().all(|b| b.is_ascii_digit())) {
            Some(app) => format!("steam://store/{app}"),
            None => format!("steam://openurl/{url}"),
        }),
        ("epic", _) => url.path().split_once("/p/").and_then(|(_, s)| slug(s)).map(|s| format!("com.epicgames.launcher://store/p/{s}")),
        ("gog", "native") => numeric(id).map(|i| format!("goggalaxy://openGameView/{i}")),
        ("gog", "heroic") => numeric(id).map(|i| format!("heroic://launch/gog/{i}")),
        ("gog", "lutris") => url.path().split_once("/game/").and_then(|(_, s)| slug(s)).map(|s| format!("lutris:{}", s.replace('_', "-"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links() {
        let gog = Url::parse("https://www.gog.com/en/game/the_witcher_3_wild_hunt").unwrap();
        assert_eq!(link("gog", "native", &gog, Some("gog-1207664643")).unwrap(), "goggalaxy://openGameView/1207664643");
        assert_eq!(link("gog", "heroic", &gog, Some("1207664643")).unwrap(), "heroic://launch/gog/1207664643");
        assert_eq!(link("gog", "lutris", &gog, None).unwrap(), "lutris:the-witcher-3-wild-hunt");
        assert!(link("gog", "heroic", &gog, Some("gp-12")).is_none());
        let epic = Url::parse("https://store.epicgames.com/en-US/p/bad-cheese-4256d9").unwrap();
        assert_eq!(link("epic", "native", &epic, None).unwrap(), "com.epicgames.launcher://store/p/bad-cheese-4256d9");
        let steam = Url::parse("https://store.steampowered.com/app/60/Ricochet/").unwrap();
        assert_eq!(link("steam", "native", &steam, None).unwrap(), "steam://store/60");
        let sale = Url::parse("https://store.steampowered.com/specials").unwrap();
        assert_eq!(link("steam", "native", &sale, None).unwrap(), "steam://openurl/https://store.steampowered.com/specials");
        // remote data can't aim a launcher at another site or smuggle odd characters into a slug
        let evil = Url::parse("https://evil.example/app/60/").unwrap();
        assert!(link("steam", "native", &evil, None).is_none());
        let odd = Url::parse("https://www.gog.com/en/game/a%20b;c").unwrap();
        assert!(link("gog", "lutris", &odd, None).is_none());
    }
}
