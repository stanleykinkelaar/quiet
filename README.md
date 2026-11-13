# Quiet

[![CI](https://github.com/stanleykinkelaar/Quiet/actions/workflows/ci.yml/badge.svg)](https://github.com/stanleykinkelaar/Quiet/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/stanleykinkelaar/Quiet)](https://github.com/stanleykinkelaar/Quiet/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

**A calm to-do app that asks one question at a time.**

Most to-do apps show you everything at once: overdue warnings, priorities, counters, red badges. Quiet shows you today, and very little else. It is made for people who get overwhelmed by their own list, ADHD brains included.

Free and open source, for Windows, macOS and Linux. No account, no cloud, no tracking.

## What it does

* **Plan your day in two minutes.** Press P and Quiet asks about one task at a time: today, not today, or already done. At five it says the day is full.
* **Start without deciding.** The top task in Today is marked Now. Finish it and the next one moves up.
* **Catch a thought from anywhere.** Press Ctrl+Alt+Space, type, press Enter. It waits in your Inbox until you are ready.
* **Nothing turns red.** No overdue, no priorities, no streaks. What you did not finish waits quietly under "From earlier".
* **See what you did.** The Done list shows what you finished, day by day.

## Install

**macOS and Linux**, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.ps1 | iex
```

Rather not pipe a script into your shell? Download the app from the [latest release](https://github.com/stanleykinkelaar/Quiet/releases/latest) and check it against `SHA256SUMS`.

Quiet starts in your tray (the menu bar on macOS) and keeps itself up to date. No admin rights needed. Runs on Windows 10 and 11, macOS 12 or newer, and Linux on x86_64. On Linux, bind a key to `quiet --capture` for quick capture; the [guide](docs/GUIDE.md#linux-and-omarchy) shows how.

## Your first five minutes

1. Press **Ctrl+Alt+Space**, type whatever is on your mind and press Enter. Do it again for the next thing.
2. Open Quiet and press **P** to plan today.
3. Do the task marked **Now**. Press **Space** when it is done.
4. Tomorrow, press **P** again. Anything unfinished is waiting for you, not blaming you.

Every key and feature is in the [user guide](docs/GUIDE.md).

## Your list stays yours

Your list is one file on your own computer. The only thing Quiet asks the internet is whether a new version exists, and you can switch that off. Before an update changes your list, Quiet keeps a copy, and you can export everything at any time.

## Contributing

Found a bug or something that feels loud? [Open an issue](https://github.com/stanleykinkelaar/Quiet/issues). Quiet is built around what it leaves out, so for a new feature, please open an issue to talk about it before writing code.

Quiet is written in Rust with [GPUI](https://www.gpui.rs) and SQLite. [Development](docs/DEVELOPMENT.md) explains how to build, test and release it.

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## License

[MIT](LICENSE)
