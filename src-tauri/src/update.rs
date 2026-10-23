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

/// The host folders `XDG_CONFIG_HOME` / `XDG_DATA_HOME` point at: the sandbox's `HOST_XDG_*`
/// override when set, else the home defaults. `env` is `std::env::var_os` in the caller.
#[cfg(any(target_os = "linux", test))]
fn host_dirs(home: &std::path::Path, env: impl Fn(&str) -> Option<std::ffi::OsString>) -> [(String, std::path::PathBuf); 2] {
    [("XDG_CONFIG_HOME", ".config"), ("XDG_DATA_HOME", ".local/share")].map(|(var, default)| {
        let host = env(&format!("HOST_{var}")).filter(|p| !p.is_empty()).map_or_else(|| home.join(default), Into::into);
        (var.to_string(), host)
    })
}

/// Flatpak points `XDG_CONFIG_HOME` / `XDG_DATA_HOME` into `~/.var/app/<id>`. Point them back at the
/// host's folders (the manifest grants `dealdrop/` and `com.dealdrop.desktop/` in them), so config.json,
/// the autostart entry and the webview's storage are the same files the AppImage used.
/// Call before any thread starts.
#[cfg(target_os = "linux")]
pub fn use_host_dirs() {
    if !in_flatpak() { return; }
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    for (var, host) in host_dirs(&home, |k| std::env::var_os(k)) {
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

/// The host command that installs `bundle` over this copy (`installation` is `--user`/`--system`).
#[cfg(any(target_os = "linux", test))]
fn install_command(installation: &str, bundle: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("flatpak-spawn");
    cmd.args(["--host", "flatpak", "install", installation, "-y", "--reinstall", "--bundle"]).arg(bundle);
    cmd
}

/// The host helper that waits for this instance to exit and starts the new copy. Backgrounded on
/// the host so it outlives this sandbox; gives up after 60 s rather than start a second copy.
#[cfg(any(target_os = "linux", test))]
fn restart_command(id: &str) -> std::process::Command {
    use std::process::Stdio;
    const SCRIPT: &str = r#"(i=0; while flatpak ps --columns=instance | grep -qx "$1"; do [ $i -ge 300 ] && exit 1; sleep 0.2; i=$((i+1)); done; exec flatpak run "$2") >/dev/null 2>&1 &"#;
    let mut cmd = std::process::Command::new("flatpak-spawn");
    cmd.args(["--host", "sh", "-c", SCRIPT, "sh", id, APP_ID])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd
}

/// Installs the downloaded bundle over this copy and schedules the restart. The caller exits next.
#[cfg(target_os = "linux")]
pub fn install(bundle: &[u8]) -> Result<(), String> {
    let info = info().ok_or("DealDrop isn't running as a Flatpak")?;
    // The sandbox's cache folder (~/.var/app/<id>/cache) has the same path on the host.
    let dir = std::env::var_os("XDG_CACHE_HOME").map(std::path::PathBuf::from).ok_or("No cache folder to download into")?;
    let file = dir.join("dealdrop-update.flatpak");
    std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, bundle)).map_err(|e| {
        let _ = std::fs::remove_file(&file); // a half-written bundle
        format!("Couldn't save the update to {}: {e}", file.display())
    })?;
    // No --noninteractive: it also stops polkit from asking for the password a system-wide install needs.
    let out = install_command(installation(&info), &file).output()
        .map_err(|e| format!("Couldn't start Flatpak: {e}"));
    let _ = std::fs::remove_file(&file);
    let out = out?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).trim().to_string();
        return Err(format!("Flatpak couldn't install the update: {err}"));
    }
    let id = instance_id(&info).ok_or("Updated. Quit DealDrop from the tray and start it again to use the new version")?;
    restart_command(id).status()
        .map_err(|e| format!("Updated, but couldn't restart DealDrop ({e}). Quit it from the tray and start it again"))?;
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
        // No app-path at all (an older runtime): not provably in the user's home, so --system asks for the password
        assert_eq!(installation("[Instance]\ninstance-id=x\n"), "--system");
    }

    #[test]
    fn host_dirs_follow_the_host_overrides_and_fall_back_to_home() {
        let home = std::path::Path::new("/home/me");
        let get = |k: &str| match k {
            "HOST_XDG_CONFIG_HOME" => Some("/host/cfg".into()),
            _ => None,
        };
        let dirs = host_dirs(home, get);
        assert_eq!(dirs[0].0, "XDG_CONFIG_HOME");
        assert_eq!(dirs[0].1, std::path::Path::new("/host/cfg"));
        assert_eq!(dirs[1].1, home.join(".local/share")); // no override: the home default
        // An empty override is no override
        let both = host_dirs(home, |k| Some(if k == "HOST_XDG_DATA_HOME" { "".into() } else { "/h".into() }));
        assert_eq!(both[0].1, std::path::Path::new("/h"));
        assert_eq!(both[1].1, home.join(".local/share"));
    }

    #[test]
    fn install_command_installs_the_bundle_without_noninteractive() {
        let bundle = std::path::Path::new("/cache/dealdrop-update.flatpak");
        for flag in ["--user", "--system"] {
            let cmd = install_command(flag, bundle);
            assert_eq!(cmd.get_program(), "flatpak-spawn");
            let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
            assert_eq!(args, vec!["--host", "flatpak", "install", flag, "-y", "--reinstall", "--bundle", "/cache/dealdrop-update.flatpak"]);
            // `--noninteractive` also stops polkit from asking for the password (see AGENTS.md)
            assert!(!args.iter().any(|a| a.contains("noninteractive")));
        }
    }

    #[test]
    fn restart_command_waits_for_this_instance_then_runs_the_app() {
        let cmd = restart_command("1234567");
        assert_eq!(cmd.get_program(), "flatpak-spawn");
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(&args[..3], &["--host", "sh", "-c"]);
        let script = &args[3];
        assert!(script.contains("flatpak ps") && script.contains("flatpak run"), "{script}");
        assert_eq!(&args[4..], &["sh", "1234567", APP_ID]); // $1 = instance to wait for, $2 = the app
    }
}
