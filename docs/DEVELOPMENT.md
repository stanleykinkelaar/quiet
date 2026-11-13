# Developing Quiet

How to build, test and release Quiet. For using it, see the [README](../README.md) and the [user guide](GUIDE.md).

Quiet is pure Rust: GPUI and gpui-component for the interface, SQLite (bundled) for storage. No accounts, no telemetry.

## What you need

On Windows 10 or 11 (x64):

1. Visual Studio 2022 Build Tools with the "Desktop development with C++" workload. It includes the MSVC compiler and the Windows 11 SDK. The SDK matters: release builds compile GPUI's shaders with its `fxc.exe`.

   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

2. Rust through rustup, stable toolchain (tested with Rust 1.98).

   ```powershell
   winget install Rustlang.Rustup
   rustup default stable-x86_64-pc-windows-msvc
   ```

3. Optional, for the installer: Inno Setup 6.

   ```powershell
   winget install JRSoftware.InnoSetup
   ```

Open a new terminal after installing so `cargo` and the compiler are on your PATH.

On macOS: Xcode Command Line Tools (`xcode-select --install`) and Rust through rustup.

On Linux: Rust through rustup, a C compiler, and the development headers GPUI links against. On Arch (Omarchy):

```sh
sudo pacman -S --needed base-devel libxkbcommon libxkbcommon-x11 libxcb wayland vulkan-icd-loader fontconfig freetype2
```

On Debian or Ubuntu:

```sh
sudo apt install build-essential libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libx11-xcb-dev libwayland-dev libfreetype-dev libfontconfig-dev libvulkan-dev
```

## Run it while developing

```powershell
cargo run
```

The first build takes a few minutes (GPUI is large). Later builds take seconds.

Run the storage tests (they do not need a window):

```powershell
cargo test -p quiet-store
```

## Build a release .exe

```powershell
cargo build --release
```

The result is `target\release\quiet.exe`, one file with no runtime to install. You can copy it anywhere and run it. On its first start, a release build turns on "Start with Windows" for itself (you can switch it off in Settings). Release builds write errors to `%APPDATA%\Quiet\quiet.log`, because they have no console.

If the build stops with "Failed to find fxc.exe", the Windows SDK is missing. Add it in the Visual Studio Installer, or point `GPUI_FXC_PATH` at an existing `fxc.exe`.

## Build the installer

```powershell
cargo build --release
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\quiet.iss
```

This writes `target\installer\QuietSetup-0.1.0.exe`. It installs for the current user only (no admin prompt) into `%LOCALAPPDATA%\Programs\Quiet`, adds a Start menu entry, and offers to start Quiet. Uninstalling removes the program, its startup entry and its notification registration. Your tasks in `%APPDATA%\Quiet` stay.

## Releases

Releases are automatic. Every push to `main` builds Windows, macOS and Linux on GitHub Actions (`.github/workflows/release.yml`). If the new commits since the last release include a `fix`, `perf`, `feat` or breaking change, the workflow also bumps the version, tags it and publishes a GitHub release with release notes made from those commits.

| Commit | Release |
| --- | --- |
| `fix: ...`, `perf: ...` | patch, 0.1.0 to 0.1.1 |
| `feat: ...` | minor, 0.1.0 to 0.2.0 |
| `feat!: ...` or a `BREAKING CHANGE:` footer | major, 1.2.0 to 2.0.0 (minor while below 1.0.0) |
| `chore`, `docs`, `ci`, `refactor`, `test`, `style` | builds, but no release |

The version is bumped only after all three builds succeed, by a `chore(release): vX.Y.Z` commit on `main` made by GitHub Actions. Pull before you push your next change. To jump to a version yourself (for example 1.0.0), set it in `Cargo.toml`; a higher version there wins.

The release gets `QuietSetup.exe`, `Quiet-macos.zip` (a universal app), `quiet-linux-x86_64.tar.gz` and `SHA256SUMS`, which the in-app updater checks every download against. The names carry no version, so the install scripts always fetch the newest one. Builds that do not release are still downloadable as workflow artifacts from the run's page. Pull requests are build checked by `.github/workflows/ci.yml`.

## Changing the database

The schema lives in `MIGRATIONS` in `store/src/lib.rs`. Each entry runs once, in order, and `user_version` records how far a database got.

* Add a new entry at the end. Never edit or reorder one that has shipped: databases out there already ran it.
* An older Quiet refuses a database whose `user_version` is higher than it knows (`NewerDatabase`), before touching it, and says so in a notification. So a migration may change anything, but going back to an older version always means restoring a backup.
* Before a migration runs on an existing database, `Store::open` copies it to the `backups` folder. If that fails, nothing is migrated.
* `every_old_schema_upgrades_and_is_backed_up_first` builds a database at every older schema version, upgrades it and checks nothing was lost. A new migration is covered automatically; run `cargo test -p quiet-store`.

## Project layout

```text
store/            the database crate: tasks, projects, settings, day rollover (no UI)
src/main.rs       start up, single instance, windows, tray and hotkey wiring
src/ui/           the main window: Today, Inbox, Projects, Done, Settings, edit dialogs
src/capture.rs    the quick capture window
src/system.rs     tray icon and global hotkey (per system)
src/reminders.rs  the once a day rollover and reminder notifications
src/platform.rs   single instance, start at sign-in, notification check
src/update.rs     automatic and manual updates: check, download, verify, install
build.rs          draws the app icon into the Windows exe
installer/        Inno Setup script (Windows)
packaging/        icon tool, macOS bundle script, Linux tarball and .desktop file
install.sh        installer for macOS and Linux
install.ps1       installer for Windows
```

Versions are pinned on purpose. `gpui-kit = "=0.9.1"` (Longbridge's crate that ships gpui-component) pins GPUI to the exact snapshot it was built with, `gpui-pre 0.3.12`. GPUI changes often, so upgrade them together and expect API changes.

## Developing from WSL

You can cross-compile debug builds from WSL with `cargo xwin` (it needs LLVM's `clang-cl`, `lld-link` and `llvm-rc` on PATH):

```bash
cargo xwin build --target x86_64-pc-windows-msvc
./target/x86_64-pc-windows-msvc/debug/quiet.exe
```

Two limits apply. Debug builds of GPUI load their shader source from the crate's path on disk, so start the exe from inside WSL as shown. And release builds must be made on Windows, because the shaders are compiled with Windows' `fxc.exe`.
