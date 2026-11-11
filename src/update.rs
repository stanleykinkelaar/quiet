//! Updating from inside the app. When asked (the button in Settings), Quiet
//! looks up the newest GitHub release, downloads this system's file, checks
//! it against the release's SHA256SUMS, installs it and starts again.
//!
//! Quiet never goes online by itself: nothing here runs until you press the
//! button.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, bail};
use gpui_kit::{App, AppContext as _, Global, SharedString};
use sha2::{Digest as _, Sha256};

use crate::data;

const REPO: &str = "stanleykinkelaar/Quiet";
/// Set by build.rs: the released version, or Cargo.toml's for local builds.
pub const CURRENT: &str = env!("QUIET_VERSION");

/// The release file for this system. The names carry no version.
#[cfg(windows)]
const ASSET: &str = "QuietSetup.exe";
#[cfg(target_os = "macos")]
const ASSET: &str = "Quiet-macos.zip";
#[cfg(target_os = "linux")]
const ASSET: &str = "quiet-linux-x86_64.tar.gz";

#[derive(Clone, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Installing(String),
    Failed(SharedString),
}

/// Lives as a global, so a download carries on when the window is closed.
pub struct Updater(State);
impl Global for Updater {}

pub fn init(cx: &mut App) {
    cx.set_global(Updater(State::Idle));
}

pub fn state(cx: &App) -> State {
    cx.global::<Updater>().0.clone()
}

fn set(state: State, cx: &mut App) {
    cx.global_mut::<Updater>().0 = state;
}

/// Why this copy cannot update itself, if it cannot.
pub fn unsupported() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        Some("Development builds do not update themselves.")
    } else if cfg!(all(target_os = "linux", not(target_arch = "x86_64"))) {
        Some("Releases are built for x86_64 Linux only, so this copy cannot update itself.")
    } else {
        None
    }
}

pub fn check(cx: &mut App) {
    set(State::Checking, cx);
    let latest = cx.background_spawn(async { latest_version() });
    cx.spawn(async move |cx| {
        let state = match latest.await {
            Ok(version) if is_newer(&version, CURRENT) => State::Available(version),
            Ok(_) => State::UpToDate,
            Err(err) => {
                data::log_error(&err.context("checking for updates"));
                State::Failed("Could not reach GitHub. Check your connection and try again.".into())
            }
        };
        cx.update(|cx| set(state, cx));
    })
    .detach();
}

/// Downloads and installs `version`, then quits. A small helper process
/// waits for Quiet to exit and starts the new version.
pub fn install(version: String, cx: &mut App) {
    set(State::Installing(version.clone()), cx);
    let installed = cx.background_spawn(async move { download_and_apply(&version) });
    cx.spawn(async move |cx| match installed.await {
        Ok(()) => cx.update(|cx| cx.quit()),
        Err(err) => {
            let message = format!("The update did not install: {err:#}");
            data::log_error(&err.context("installing the update"));
            cx.update(|cx| set(State::Failed(message.into()), cx));
        }
    })
    .detach();
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .user_agent(concat!("Quiet/", env!("QUIET_VERSION")))
        .build()
        .into()
}

fn latest_version() -> anyhow::Result<String> {
    let body = agent()
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .read_to_string()?;
    let release: serde_json::Value = serde_json::from_str(&body)?;
    let tag = release["tag_name"].as_str().context("the release has no tag")?;
    Ok(tag.trim_start_matches('v').to_string())
}

/// Compares `x.y.z` versions. Anything unreadable is not newer.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let mut parts = v.trim().splitn(3, '.').map(|p| p.parse().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    }
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

fn download_and_apply(version: &str) -> anyhow::Result<()> {
    let base = format!("https://github.com/{REPO}/releases/download/v{version}");
    let agent = agent();

    let sums = agent.get(format!("{base}/SHA256SUMS")).call()?.body_mut().read_to_string()?;
    let expected = expected_sum(&sums, ASSET).context("the release has no checksum for this system")?;

    let dir = std::env::temp_dir().join(format!("quiet-update-{version}"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(ASSET);
    let actual = download(&agent, &format!("{base}/{ASSET}"), &file)?;
    if actual != expected {
        bail!("the download is damaged (checksum mismatch), nothing was changed");
    }
    apply(&file, &dir)
}

/// Finds `name` in `sha256sum` output: `<hex>  <name>` per line.
fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim_start().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

/// Streams `url` into `dest` and returns its SHA-256 as hex.
fn download(agent: &ureq::Agent, url: &str, dest: &Path) -> anyhow::Result<String> {
    let mut response = agent.get(url).call()?;
    let mut reader = response.body_mut().as_reader();
    let mut out = std::fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    out.sync_all()?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Windows: once Quiet has exited, run the installer silently (it updates
/// the per-user install in place), then start Quiet again.
#[cfg(windows)]
fn apply(setup: &Path, dir: &Path) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let local = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    let installed = std::path::PathBuf::from(local).join(r"Programs\Quiet\quiet.exe");
    let current = std::env::current_exe()?;
    // PowerShell single-quoted strings only need ' doubled.
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "''"));
    let script = format!(
        "Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue; \
         $p = Start-Process -FilePath {setup} -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Wait -PassThru; \
         if ($p.ExitCode -eq 0) {{ Start-Process {installed} }} else {{ Start-Process {current} }}; \
         Remove-Item -Recurse -Force {dir} -ErrorAction SilentlyContinue",
        pid = std::process::id(),
        setup = quote(setup),
        installed = quote(&installed),
        current = quote(&current),
        dir = quote(dir),
    );
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("could not start the installer")?;
    Ok(())
}

/// macOS: swap Quiet.app for the new one (a running app can be replaced),
/// then reopen it once this copy has exited.
#[cfg(target_os = "macos")]
fn apply(zip: &Path, dir: &Path) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let bundle = exe
        .ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .context("Quiet is not running from an app bundle")?
        .to_path_buf();
    let parent = bundle.parent().context("the app has no folder")?;

    let unpacked = dir.join("unpacked");
    run(std::process::Command::new("ditto").arg("-x").arg("-k").arg(zip).arg(&unpacked))?;
    let fresh = unpacked.join("Quiet.app");
    anyhow::ensure!(fresh.is_dir(), "the download has no Quiet.app");

    // Stage next to the old app, so the final swap is a rename on one disk.
    let staged = parent.join(".Quiet-update.app");
    let old = parent.join(".Quiet-old.app");
    std::fs::remove_dir_all(&staged).ok();
    std::fs::remove_dir_all(&old).ok();
    run(std::process::Command::new("ditto").arg(&fresh).arg(&staged))
        .with_context(|| format!("{} is not writable", parent.display()))?;
    std::fs::rename(&bundle, &old)?;
    if let Err(err) = std::fs::rename(&staged, &bundle) {
        std::fs::rename(&old, &bundle).ok();
        return Err(err.into());
    }
    std::fs::remove_dir_all(&old).ok();
    std::fs::remove_dir_all(dir).ok();
    std::process::Command::new("xattr").arg("-dr").arg("com.apple.quarantine").arg(&bundle).status().ok();
    relaunch(std::process::Command::new("open").arg(&bundle))
}

/// Linux: replace the binary by rename (the running copy keeps its old
/// file), then start the new one once this copy has exited.
#[cfg(target_os = "linux")]
fn apply(tarball: &Path, dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let exe = std::env::current_exe()?;
    run(std::process::Command::new("tar").arg("-xzf").arg(tarball).arg("-C").arg(dir))?;
    let fresh = dir.join("quiet/quiet");
    anyhow::ensure!(fresh.is_file(), "the download has no quiet binary");

    let staged = exe.with_file_name(".quiet.new");
    std::fs::copy(&fresh, &staged).with_context(|| {
        format!("{} is not writable, update Quiet the way you installed it", exe.parent().unwrap_or(&exe).display())
    })?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&staged, &exe)?;
    std::fs::remove_dir_all(dir).ok();
    relaunch(&mut std::process::Command::new(&exe))
}

#[cfg(unix)]
fn run(command: &mut std::process::Command) -> anyhow::Result<()> {
    let status = command.status().with_context(|| format!("could not run {command:?}"))?;
    anyhow::ensure!(status.success(), "{command:?} failed ({status})");
    Ok(())
}

/// Runs `command` once this process has exited, from a detached shell, so
/// the new copy does not hand itself to the old one and exit.
#[cfg(unix)]
fn relaunch(command: &mut std::process::Command) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt as _;

    let program = command.get_program().to_owned();
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; exec "$@""#)
        .arg(std::process::id().to_string())
        .arg(program)
        .args(command.get_args())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .context("could not schedule the restart")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
    }

    #[test]
    fn finds_checksums() {
        let sums = "AbC123  QuietSetup.exe\ndef456 *Quiet-macos.zip\n";
        assert_eq!(expected_sum(sums, "QuietSetup.exe").as_deref(), Some("abc123"));
        assert_eq!(expected_sum(sums, "Quiet-macos.zip").as_deref(), Some("def456"));
        assert_eq!(expected_sum(sums, "quiet-linux-x86_64.tar.gz"), None);
    }
}
