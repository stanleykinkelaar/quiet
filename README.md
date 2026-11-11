# Quiet

A small, calm to-do app for Windows. It is built around what it leaves out.

Quiet has an Inbox, a few flat projects, a Today list you pick yourself, and a Done list that shows what you finished. There are no priorities, due dates, labels, overdue warnings or counters (apart from "done today").

Pure Rust: GPUI and gpui-component for the interface, SQLite (bundled) for storage. No accounts, no network, no telemetry.

## Using it

| Key | What it does |
| --- | --- |
| Ctrl+Alt+Space | Quick capture from anywhere (changeable in Settings) |
| 1, 2, 3, 4 | Today, Inbox, Projects, Done |
| Ctrl+, | Settings |
| N | New task (or new project on the Projects screen) |
| Enter | Edit the selected task (open the selected project) |
| Space | Complete, or undo a completion |
| T | Put the task in Today, or take it out |
| A | Make it the project's next action (inside a project) |
| E | Edit the project (name, colour, archive) |
| Delete | Delete the task (an Undo note appears) |
| Up, Down | Move the selection |
| Alt+Up, Alt+Down | Reorder Today (dragging works too) |
| Esc | Leave a text field, or go back from a project |

How the day works:

* Today is yours to fill. Past five tasks you get a gentle note, nothing more.
* On the first launch of a new day, unfinished Today tasks go back to their project or the Inbox. Nothing turns red and nothing is counted.
* Repeating tasks (every day, every weekday, or weekly on one day) come into Today on their days.
* A reminder is a time of day. It rings once, as a Windows notification, on days the task is in Today. Setting a reminder puts the task in Today.
* Closing the window keeps Quiet in the tray. Use the tray menu's Quit to really quit.

Your data lives in `%APPDATA%\Quiet\quiet.db`. Set the environment variable `QUIET_DB` to a different path to try things without touching your real list.

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

## Project layout

```text
store/            the database crate: tasks, projects, settings, day rollover (no UI)
src/main.rs       start up, single instance, windows, tray and hotkey wiring
src/ui/           the main window: Today, Inbox, Projects, Done, Settings, edit dialogs
src/capture.rs    the quick capture window
src/system.rs     tray icon and global hotkey
src/reminders.rs  the once a day rollover and reminder notifications
src/platform.rs   single instance, start with Windows, notification check
build.rs          draws the app icon into the exe
installer/        Inno Setup script
```

Versions are pinned on purpose. `gpui-kit = "=0.9.1"` (Longbridge's crate that ships gpui-component) pins GPUI to the exact snapshot it was built with, `gpui-pre 0.3.12`. GPUI changes often, so upgrade them together and expect API changes.

## Developing from WSL

You can cross-compile debug builds from WSL with `cargo xwin` (it needs LLVM's `clang-cl`, `lld-link` and `llvm-rc` on PATH):

```bash
cargo xwin build --target x86_64-pc-windows-msvc
./target/x86_64-pc-windows-msvc/debug/quiet.exe
```

Two limits apply. Debug builds of GPUI load their shader source from the crate's path on disk, so start the exe from inside WSL as shown. And release builds must be made on Windows, because the shaders are compiled with Windows' `fxc.exe`.
