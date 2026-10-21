//! "Start DealDrop when I log in". The OS entry is the only record of the choice (nothing in
//! Settings): an XDG autostart file on Linux, a `Run` registry value on Windows, a LaunchAgent on
//! macOS. Each starts the app with `--hidden`, so it comes up in the tray.

#[cfg(any(target_os = "linux", target_os = "macos", test))]
use std::path::{Path, PathBuf};

pub const HIDDEN: &str = "--hidden";

/// The path to start. Inside an AppImage `current_exe` is the temporary /tmp/.mount_* copy, so
/// the AppImage file itself (`APPIMAGE`) is used instead.
fn exe() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if let Some(p) = std::env::var_os("APPIMAGE").filter(|p| !p.is_empty()) {
        return Ok(p.to_string_lossy().into_owned());
    }
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).map_err(|e| format!("Couldn't find DealDrop's own path: {e}"))
}

/// `dir/autostart/dealdrop.desktop`, `dir` being `$XDG_CONFIG_HOME` (or `~/.config`).
#[cfg(any(target_os = "linux", test))]
fn desktop_path(config: &Path) -> PathBuf { config.join("autostart").join("dealdrop.desktop") }

/// Desktop-entry `Exec` argument: quoted, with the spec's escapes (backslashes are doubled once
/// for the quoting rule and again for the string-value rule; `%` is a field code otherwise).
#[cfg(any(target_os = "linux", test))]
fn exec_arg(path: &str) -> String {
    let mut s = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' => { s.push_str("\\\\"); s.push(c); }
            '\\' => s.push_str("\\\\\\\\"),
            '%' => s.push_str("%%"),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// `Icon` is a file path: an AppImage installs no themed `dealdrop` icon, and its mount
/// (`$APPDIR`) is gone once it exits, so `set` copies the icon next to config.json.
#[cfg(any(target_os = "linux", test))]
fn desktop_entry(exe: &str, icon: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=DealDrop\nExec={} {HIDDEN}\nIcon={}\nX-GNOME-Autostart-enabled=true\n",
        exec_arg(exe), icon.display()
    )
}

#[cfg(target_os = "linux")]
const ICON: &[u8] = include_bytes!("../icons/128x128.png");

/// Matches `identifier` in tauri.conf.json.
#[cfg(any(target_os = "macos", test))]
const AGENT_LABEL: &str = "com.dealdrop.desktop";

#[cfg(any(target_os = "macos", test))]
fn launch_agent(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>{HIDDEN}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#)
}

/// `$XDG_CONFIG_HOME` or `~/.config`: holds both `autostart/` and DealDrop's own `dealdrop/`.
#[cfg(target_os = "linux")]
fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

#[cfg(target_os = "macos")]
fn entry() -> Option<(PathBuf, fn(&str) -> String)> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some((home.join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")), launch_agent))
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn write(path: &Path, text: impl AsRef<[u8]>) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|old| old == text.as_ref()) { return Ok(()); } // unchanged: leave it alone
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?; }
    std::fs::write(path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Couldn't remove {}: {e}", path.display())),
        _ => Ok(()),
    }
}

/// Desktop-entry `Exec=` line back to the path `exec_arg` quoted; None if it isn't one of ours.
#[cfg(any(target_os = "linux", test))]
fn exec_path(line: &str) -> Option<String> {
    let mut rest = line.strip_prefix("Exec=\"")?;
    let mut out = String::new();
    loop {
        if let Some(r) = rest.strip_prefix(r"\\\\") { out.push('\\'); rest = r; }
        else if let Some(r) = rest.strip_prefix(r"\\").or_else(|| rest.strip_prefix("%")) {
            let c = r.chars().next()?;
            out.push(c); rest = &r[c.len_utf8()..];
        } else {
            let c = rest.chars().next()?;
            if c == '"' { return Some(out); }
            out.push(c); rest = &rest[c.len_utf8()..];
        }
    }
}

/// The entry with only its `Exec=` line pointed at `exe`, when the app it records is gone (an AppImage
/// update renames the file). Everything else is kept, e.g. `Hidden=true` or
/// `X-GNOME-Autostart-enabled=false` from the desktop's own autostart settings. A recorded app that
/// still exists is left alone, so trying another copy doesn't take the entry over, unless both are
/// release AppImages in the same folder: AppImageUpdate keeps the old file next to the new one. Then an
/// older running copy (by the `DealDrop_<version>_…` filename) doesn't take it back.
#[cfg(any(target_os = "linux", test))]
fn repaired_entry(text: &str, exe: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let old = text.lines().find_map(exec_path)?;
    let release = |p: &str| p.rsplit_once('/').filter(|(_, f)| f.starts_with("DealDrop_") && f.ends_with(".AppImage")).map(|(dir, _)| dir.to_owned());
    // "0.10.0" -> [0, 10, 0], so versions compare as numbers; an unparsable one never blocks the update.
    let version = |p: &str| -> Option<Vec<u32>> { p.rsplit_once('/')?.1.strip_prefix("DealDrop_")?.split_once('_')?.0.split('.').map(|n| n.parse().ok()).collect() };
    let older = matches!((version(exe), version(&old)), (Some(run), Some(rec)) if run < rec);
    let update = !older && release(&old).is_some_and(|dir| release(exe) == Some(dir));
    if old == exe || (exists(&old) && !update) || exe.starts_with("/tmp/.mount_") { return None; }
    let exec = format!("Exec={} {HIDDEN}", exec_arg(exe));
    Some(text.lines().map(|l| if l.starts_with("Exec=") { exec.as_str() } else { l }).collect::<Vec<_>>().join("\n") + "\n")
}

/// Held across reading and writing the entry, so the startup repair and the Settings toggle can't interleave.
#[cfg(target_os = "linux")]
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// At startup, repair an entry whose app is gone (Linux: AppImage updates rename the file), which
/// would otherwise start nothing while the toggle still shows On. Release builds only, so
/// `cargo tauri dev` never points it at a dev binary. Windows (MSI) and macOS (/Applications) install
/// to a fixed path, and a Mac app run from the DMG or a translocated path must not take it over.
#[cfg(target_os = "linux")]
pub fn refresh() {
    if cfg!(debug_assertions) { return; }
    let _lock = LOCK.lock();
    let Some(path) = config_home().map(|c| desktop_path(&c)) else { return };
    let (Ok(text), Ok(exe)) = (std::fs::read_to_string(&path), exe()) else { return };
    if let Some(fixed) = repaired_entry(&text, &exe, |p| Path::new(p).exists()) {
        if let Err(e) = write(&path, fixed) { eprintln!("autostart refresh failed: {e}"); }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn refresh() {}

#[cfg(target_os = "linux")]
pub fn get() -> bool { config_home().is_some_and(|c| desktop_path(&c).exists()) }

#[cfg(target_os = "linux")]
pub fn set(on: bool) -> Result<(), String> {
    let _lock = LOCK.lock();
    let config = config_home().ok_or("Couldn't find your home folder")?;
    let icon = config.join("dealdrop").join("autostart-icon.png");
    if !on { return remove(&desktop_path(&config)).and(remove(&icon)); }
    write(&icon, ICON)?;
    write(&desktop_path(&config), desktop_entry(&exe()?, &icon))
}

#[cfg(target_os = "macos")]
pub fn get() -> bool { entry().is_some_and(|(p, _)| p.exists()) }

#[cfg(target_os = "macos")]
pub fn set(on: bool) -> Result<(), String> {
    let (path, text) = entry().ok_or("Couldn't find your home folder")?;
    if on { write(&path, &text(&exe()?)) } else { remove(&path) }
}

#[cfg(target_os = "windows")]
const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(target_os = "windows")]
fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash
        .output()
}

#[cfg(target_os = "windows")]
pub fn get() -> bool { reg(&["query", RUN, "/v", "DealDrop"]).map(|o| o.status.success()).unwrap_or(false) }

#[cfg(target_os = "windows")]
pub fn set(on: bool) -> Result<(), String> {
    let out = if on {
        let cmd = format!("\"{}\" {HIDDEN}", exe()?);
        reg(&["add", RUN, "/v", "DealDrop", "/t", "REG_SZ", "/d", &cmd, "/f"])
    } else {
        reg(&["delete", RUN, "/v", "DealDrop", "/f"])
    }
    .map_err(|e| format!("Couldn't run reg: {e}"))?;
    // `reg delete` fails when the value is already gone; only the end state matters.
    if get() == on { Ok(()) } else { Err(format!("reg failed: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_quotes_the_path() {
        let text = desktop_entry("/home/me/My Apps/DealDrop_0.8.0_amd64.AppImage", Path::new("/home/me/.config/dealdrop/autostart-icon.png"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("\nExec=\"/home/me/My Apps/DealDrop_0.8.0_amd64.AppImage\" --hidden\n"));
        for line in ["Type=Application", "Name=DealDrop", "Icon=/home/me/.config/dealdrop/autostart-icon.png", "X-GNOME-Autostart-enabled=true"] {
            assert!(text.lines().any(|l| l == line), "missing {line}");
        }
        assert_eq!(exec_arg(r#"/a "b" $c `d` \e 100%"#), r#""/a \\"b\\" \\$c \\`d\\` \\\\e 100%%""#);
    }

    #[test]
    fn a_missing_app_is_repaired_and_desktop_settings_are_kept() {
        for p in ["/home/me/Apps/DealDrop_0.8.0_amd64.AppImage", r#"/a "b" $c `d` \e 100%"#, "/ünï/çødé"] {
            assert_eq!(exec_path(&format!("Exec={} --hidden", exec_arg(p))).as_deref(), Some(p));
        }
        let old = desktop_entry("/apps/DealDrop_0.8.0_amd64.AppImage", Path::new("/i.png")).replace("enabled=true", "enabled=false") + "Hidden=true\n";
        let new = "/apps/DealDrop_0.9.0_amd64.AppImage";
        let fixed = repaired_entry(&old, new, |_| false).unwrap();
        assert_eq!(fixed, old.replace("0.8.0", "0.9.0")); // only Exec= changed; the user's "off" stays
        assert!(repaired_entry(&old, new, |_| true).is_some()); // AppImageUpdate kept the old file next to it
        assert!(repaired_entry(&old, "/home/me/Downloads/DealDrop_0.9.0_amd64.AppImage", |_| true).is_none()); // a copy elsewhere
        assert!(repaired_entry(&old, "/apps/dealdrop-test", |_| true).is_none()); // not a release AppImage
        let newer = desktop_entry("/apps/DealDrop_0.10.0_amd64.AppImage", Path::new("/i.png"));
        assert!(repaired_entry(&newer, "/apps/DealDrop_0.9.1_amd64.AppImage", |_| true).is_none()); // an older copy (as numbers, not text)
        assert!(repaired_entry(&newer, "/apps/DealDrop_0.9.1_amd64.AppImage", |_| false).is_some()); // ...unless the recorded one is gone
        assert!(repaired_entry(&old, "/apps/DealDrop_0.10.0_amd64.AppImage", |_| true).is_some()); // 0.8.0 -> 0.10.0 is newer
        assert!(repaired_entry(&newer, "/apps/DealDrop_dev_amd64.AppImage", |_| true).is_some()); // unparsable: as before
        assert!(repaired_entry(&old, "/tmp/.mount_x/dealdrop", |_| false).is_none());
        assert!(repaired_entry("[Desktop Entry]\nExec=other\n", new, |_| false).is_none());
    }

    #[test]
    fn launch_agent_runs_hidden_at_login() {
        let text = launch_agent("/Applications/Deal & Drop.app/Contents/MacOS/DealDrop");
        assert!(text.contains("<string>com.dealdrop.desktop</string>"));
        assert!(text.contains("<array>\n    <string>/Applications/Deal &amp; Drop.app/Contents/MacOS/DealDrop</string>\n    <string>--hidden</string>\n  </array>"));
        assert!(text.contains("<key>RunAtLoad</key>\n  <true/>"));
    }

    #[test]
    fn the_launch_agent_escapes_the_app_path() {
        let text = launch_agent("/Applications/Deal & <Drop>.app/Contents/MacOS/DealDrop");
        assert!(text.contains("/Applications/Deal &amp; &lt;Drop&gt;.app/Contents/MacOS/DealDrop"));
        assert!(!text.contains("<Drop>"));
    }

    #[test]
    fn only_our_own_exec_line_is_read_back() {
        assert_eq!(exec_path(r#"Exec="/apps/DealDrop.AppImage" --hidden"#).as_deref(), Some("/apps/DealDrop.AppImage"));
        assert!(exec_path("Exec=/apps/DealDrop.AppImage").is_none()); // not quoted
        assert!(exec_path("Exec=").is_none());
        assert!(exec_path("Exec").is_none());
        assert!(exec_path(" X-Exec=\"/apps/x\"").is_none());
    }

    #[test]
    fn the_entry_is_only_repointed_when_the_app_it_records_is_gone() {
        let old = desktop_entry("/apps/DealDrop_0.8.0_amd64.AppImage", Path::new("/i.png"));
        // The running copy is the one the entry already names
        assert!(repaired_entry(&old, "/apps/DealDrop_0.8.0_amd64.AppImage", |_| true).is_none());
        // A gone app: the entry follows the running copy, everything else is kept
        let fixed = repaired_entry(&old, "/opt/dealdrop", |_| false).unwrap();
        assert!(fixed.contains("Exec=\"/opt/dealdrop\" --hidden"));
        assert!(fixed.contains("Icon=/i.png"));
        assert!(fixed.contains("Name=DealDrop"));
        assert!(fixed.ends_with('\n'));
    }

    #[test]
    fn autostart_file_is_written_and_removed() {
        let base = std::env::temp_dir().join(format!("dealdrop-autostart-test-{}", std::process::id()));
        let path = desktop_path(&base);
        assert_eq!(path, base.join("autostart/dealdrop.desktop"));
        write(&path, desktop_entry("/opt/Deal Drop/dealdrop", &base.join("dealdrop/autostart-icon.png"))).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("Exec=\"/opt/Deal Drop/dealdrop\" --hidden"));
        // Writing the same text again (turning it on twice) does not touch the file: it works even read-only.
        let text = std::fs::read(&path).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms.clone()).unwrap();
        write(&path, &text).unwrap();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
        write(&path, desktop_entry("/opt/DealDrop_9.9.9_amd64.AppImage", &base.join("i.png"))).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("Exec=\"/opt/DealDrop_9.9.9_amd64.AppImage\" --hidden"));
        remove(&path).unwrap();
        assert!(!path.exists());
        remove(&path).unwrap(); // already gone is fine
        std::fs::remove_dir_all(&base).unwrap();
    }
}
