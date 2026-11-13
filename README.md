# Quiet

[![CI](https://github.com/stanleykinkelaar/Quiet/actions/workflows/ci.yml/badge.svg)](https://github.com/stanleykinkelaar/Quiet/actions/workflows/ci.yml)

A small, calm to-do app for Windows, macOS and Linux. It is built around what it leaves out.

Quiet has an Inbox, a few flat projects, a Today list you pick yourself, and a Done list that shows what you finished. There are no priorities, due dates, labels, overdue warnings or counters (apart from "done today").

Pure Rust: GPUI and gpui-component for the interface, SQLite (bundled) for storage. No accounts, no telemetry. The only thing Quiet asks the internet is whether a new version exists, and you can switch that off in Settings.

## Install

**macOS and Linux** (including Omarchy), in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.ps1 | iex
```

Quiet updates itself. A few times a day it asks GitHub for the newest release. When there is one, it waits until its window is closed, then downloads it, checks it against the release's checksums, installs it and starts again in the tray. Switch off Update automatically in Settings to stop that; Check for updates in Settings still works then. Running the install command again works too. Nothing needs admin rights. You can also download the files yourself from the [latest release](https://github.com/stanleykinkelaar/Quiet/releases/latest).

What goes where:

| System | App | Your tasks |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\Quiet` (uninstall in Settings, Apps) | `%APPDATA%\Quiet\quiet.db` |
| macOS | `/Applications/Quiet.app` | `~/Library/Application Support/Quiet/quiet.db` |
| Linux | `~/.local/bin/quiet`, plus a launcher entry and icon | `~/.local/share/quiet/quiet.db` |

To uninstall on macOS or Linux, add `-s -- --uninstall` after `sh`. Your tasks stay.

### Notes for macOS

The app is not notarized by Apple (that needs a paid developer account). The install script downloads with `curl`, which macOS does not quarantine, so it opens normally. If you download `Quiet-macos.zip` from the browser instead, right click Quiet.app and choose Open the first time.

Quiet lives in the menu bar. It works on Apple Silicon and Intel Macs, macOS 12 or newer.

### Notes for Linux and Omarchy

On Linux your desktop owns global shortcuts, so Quiet cannot grab Ctrl+Alt+Space itself. Bind a key to `quiet --capture` instead. On Omarchy (Hyprland), add this line to `~/.config/hypr/bindings.conf`:

```text
bindd = CTRL ALT, SPACE, Quiet capture, exec, ~/.local/bin/quiet --capture
```

On Hyprland and other wlroots desktops, quick capture appears as a centred overlay, like a launcher, so it needs no window rules. The tray icon shows in Waybar's tray (Omarchy has one). Reminders arrive through your notification daemon (mako on Omarchy).

The Linux build is x86_64 only for now. It needs Wayland or X11 with Vulkan drivers, which Omarchy and most desktops already have.

## Using it

| Key | What it does |
| --- | --- |
| Ctrl+Alt+Space | Quick capture from anywhere (changeable in Settings; on Linux, see above) |
| 1, 2, 3, 4 | Today, Inbox, Projects, Done |
| Ctrl+, (Cmd+, on macOS) | Settings |
| Ctrl+Q (Cmd+Q on macOS) | Quit (closing the window only hides it) |
| N | New task (or new project on the Projects screen) |
| Enter | Edit the selected task (open the selected project) |
| Space | Complete, or undo a completion |
| T | Put the task in Today, or take it out |
| A | Make it the project's next action (inside a project) |
| E | Edit the project (name, colour, archive) |
| K | Keep a task from an earlier day in Today |
| P | Plan today, one question at a time (on Today) |
| S | Sort the Inbox, one task at a time (on Today and Inbox) |
| F | Show project tasks in the Inbox too, or hide them again |
| Delete | Delete the task (an Undo note appears) |
| Up, Down | Move the selection |
| Alt+Up, Alt+Down | Reorder Today (dragging works too) |
| Esc | Leave a text field, or go back from a project |

How the day works:

* Today is yours to fill. Past five tasks you get a gentle note, nothing more.
* Plan today (P, or the line at the top of Today until you have) walks you through it one question at a time. First the tasks left from earlier days: keep or not today. Then what you could do: tasks with a day in the coming week, one task from each project (its next action, or else the one waiting longest), then the Inbox. Today, not today, or already done. At five it stops and says the day is full; S stops sooner. Last, you pick what to start with.
* The first task in Today is marked Now, so starting needs no decision. Finish it and the next one moves up. Reorder with Alt+Up and Alt+Down to change what comes first.
* Unfinished Today tasks stay overnight, under "From earlier". Keep them (K, or Keep all) or send them back to their project or the Inbox (T, or Send all back). Skipping a morning loses nothing. Nothing turns red and nothing is counted.
* Repeating tasks (every day, every weekday, or weekly on one day) come into Today on their days.
* A task can have a day (type fri, tomorrow, +3 or 14 nov). It comes into Today on that day, or up to a week before if you choose. Sent back, it does not keep returning. After the day it simply stays, never red.
* A reminder is a time of day, on days the task is in Today. The notification has In 10 min, In 1 hour and Done buttons. If nobody reacts, it rings once more 10 minutes later. A reminder you missed (the PC was off, or you ignored it) shows as a marked bell on the task until you click it or finish the task. Setting a reminder puts the task in Today.
* The Inbox can show the tasks of your projects too (F, or the Projects button). Each carries its project's tag, so T picks it for today and A makes it the project's next action without opening the project. Quiet remembers the choice; Sort still only asks about real Inbox tasks.
* Sort (S) shows Inbox tasks one at a time: Today, keep in Inbox, a project (1 to 9), already done, or delete. When ten or more are waiting, Today offers it in one quiet line.
* Closing the window keeps Quiet in the tray (the menu bar on macOS). Use the tray menu's Quit, or Ctrl+Q, to really quit.

Your data lives in the place listed under Install. Set the environment variable `QUIET_DB` to a different path to try things without touching your real list.

Your list is safe across updates:

* Before an update changes the database, Quiet copies it into a `backups` folder next to `quiet.db` (for example `~/.local/share/quiet/backups`). The last three copies stay. If the copy cannot be made, Quiet does not touch the database and writes why to `quiet.log`.
* Settings, Your data, Export tasks writes every task and project, done ones included, to a JSON file in your Downloads folder. It is for keeping your list somewhere else; Quiet does not read it back in.
* To go back to a copy, quit Quiet, replace `quiet.db` with the copy (rename it to `quiet.db`) and start Quiet again.

## What you need to build it

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
