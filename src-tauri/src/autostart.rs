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

/// At startup, point an existing entry at this copy of the app: an AppImage update (Gear Lever,
/// AppImageUpdate) changes the file name, and a moved app would otherwise leave the entry starting
/// nothing while the toggle still shows On. Rewrites only what differs. Release builds only, so
/// `cargo tauri dev` never points the entry at a dev binary.
pub fn refresh() {
    if !cfg!(debug_assertions) && get() {
        if let Err(e) = set(true) { eprintln!("autostart refresh failed: {e}"); }
    }
}

#[cfg(target_os = "linux")]
pub fn get() -> bool { config_home().is_some_and(|c| desktop_path(&c).exists()) }

#[cfg(target_os = "linux")]
pub fn set(on: bool) -> Result<(), String> {
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
        // Already set to this command: nothing to write.
        if reg(&["query", RUN, "/v", "DealDrop"]).is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(&cmd)) { return Ok(()); }
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
    fn launch_agent_runs_hidden_at_login() {
        let text = launch_agent("/Applications/Deal & Drop.app/Contents/MacOS/DealDrop");
        assert!(text.contains("<string>com.dealdrop.desktop</string>"));
        assert!(text.contains("<array>\n    <string>/Applications/Deal &amp; Drop.app/Contents/MacOS/DealDrop</string>\n    <string>--hidden</string>\n  </array>"));
        assert!(text.contains("<key>RunAtLoad</key>\n  <true/>"));
    }

    #[test]
    fn autostart_file_is_written_and_removed() {
        let base = std::env::temp_dir().join(format!("dealdrop-autostart-test-{}", std::process::id()));
        let path = desktop_path(&base);
        assert_eq!(path, base.join("autostart/dealdrop.desktop"));
        write(&path, desktop_entry("/opt/Deal Drop/dealdrop", &base.join("dealdrop/autostart-icon.png"))).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("Exec=\"/opt/Deal Drop/dealdrop\" --hidden"));
        // Same text again (the startup refresh) does not touch the file: it works even read-only.
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
