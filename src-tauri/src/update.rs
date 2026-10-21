//! Running as a Flatpak, and updating it in place from a GitHub release's `.flatpak` bundle.
//! The bundle is installed through the host's `flatpak` (`flatpak-spawn --host`, which needs
//! `--talk-name=org.freedesktop.Flatpak`); a host helper then waits for this copy to exit and starts
//! the new one (the running copy holds the single-instance name, so it has to go first).

pub const APP_ID: &str = "com.dealdrop.desktop";

#[cfg(target_os = "linux")]
fn info() -> Option<String> { std::fs::read_to_string("/.flatpak-info").ok() }

#[cfg(target_os = "linux")]
pub fn in_flatpak() -> bool { std::path::Path::new("/.flatpak-info").exists() }

#[cfg(not(target_os = "linux"))]
pub fn in_flatpak() -> bool { false }

/// Flatpak points `XDG_CONFIG_HOME` / `XDG_DATA_HOME` into `~/.var/app/<id>`. Point them back at the
/// host's folders (the manifest grants `dealdrop/` and `com.dealdrop.desktop/` in them), so config.json,
/// the autostart entry and the webview's storage are the same files the AppImage used.
/// Call before any thread starts.
#[cfg(target_os = "linux")]
pub fn use_host_dirs() {
    if !in_flatpak() { return; }
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    for (var, default) in [("XDG_CONFIG_HOME", ".config"), ("XDG_DATA_HOME", ".local/share")] {
        let host = std::env::var_os(format!("HOST_{var}")).filter(|p| !p.is_empty()).map_or_else(|| home.join(default), Into::into);
        std::env::set_var(var, host);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn use_host_dirs() {}

/// `--user` unless this copy was installed system-wide (`app-path=` outside `~/.local/share/flatpak`).
#[cfg(any(target_os = "linux", test))]
fn installation(info: &str) -> &'static str {
    let user = info.lines().find_map(|l| l.strip_prefix("app-path=")).is_some_and(|p| p.contains("/.local/share/flatpak/"));
    if user { "--user" } else { "--system" }
}

#[cfg(any(target_os = "linux", test))]
fn instance_id(info: &str) -> Option<&str> {
    info.lines().find_map(|l| l.strip_prefix("instance-id=")).map(str::trim).filter(|id| !id.is_empty())
}

/// Installs the downloaded bundle over this copy and schedules the restart. The caller exits next.
#[cfg(target_os = "linux")]
pub fn install(bundle: &[u8]) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let info = info().ok_or("DealDrop isn't running as a Flatpak")?;
    // The sandbox's cache folder (~/.var/app/<id>/cache) has the same path on the host.
    let dir = std::env::var_os("XDG_CACHE_HOME").map(std::path::PathBuf::from).ok_or("No cache folder to download into")?;
    let file = dir.join("dealdrop-update.flatpak");
    std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, bundle)).map_err(|e| format!("Couldn't save the update to {}: {e}", file.display()))?;
    let out = Command::new("flatpak-spawn")
        .args(["--host", "flatpak", "install", installation(&info), "-y", "--noninteractive", "--reinstall", "--bundle"])
        .arg(&file)
        .output()
        .map_err(|e| format!("Couldn't start Flatpak: {e}"));
    let _ = std::fs::remove_file(&file);
    let out = out?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).trim().to_string();
        return Err(format!("Flatpak couldn't install the update: {err}"));
    }
    let id = instance_id(&info).ok_or("Updated. Quit DealDrop from the tray and start it again to use the new version")?;
    // Backgrounded on the host so it outlives this sandbox; gives up after 60 s rather than start a second copy.
    let script = r#"(i=0; while flatpak ps --columns=instance | grep -qx "$1"; do [ $i -ge 300 ] && exit 1; sleep 0.2; i=$((i+1)); done; exec flatpak run "$2") >/dev/null 2>&1 &"#;
    Command::new("flatpak-spawn").args(["--host", "sh", "-c", script, "sh", id, APP_ID])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().map_err(|e| format!("Updated, but couldn't restart DealDrop ({e}). Quit it from the tray and start it again"))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn install(_: &[u8]) -> Result<(), String> { Err("Only the Flatpak build updates itself".into()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_info_picks_the_installation_and_instance() {
        let user = "[Application]\nname=com.dealdrop.desktop\n\n[Instance]\ninstance-id=1234567\napp-path=/home/me/.local/share/flatpak/app/com.dealdrop.desktop/x86_64/master/abc/files\n";
        assert_eq!(installation(user), "--user");
        assert_eq!(instance_id(user), Some("1234567"));
        let system = "[Instance]\ninstance-id=\napp-path=/var/lib/flatpak/app/com.dealdrop.desktop/x86_64/master/abc/files\n";
        assert_eq!(installation(system), "--system");
        assert_eq!(instance_id(system), None);
    }
}
