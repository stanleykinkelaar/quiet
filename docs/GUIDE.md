# Quiet user guide

Everything Quiet does, in one place. New to Quiet? The [README](../README.md) gets you started in five minutes.

* [Install and update](#install-and-update)
* [The four screens](#the-four-screens)
* [How the day works](#how-the-day-works)
* [Planning your day](#planning-your-day)
* [Sorting the Inbox](#sorting-the-inbox)
* [Projects](#projects)
* [Days, repeats and reminders](#days-repeats-and-reminders)
* [All keys](#all-keys)
* [Settings](#settings)
* [Your data](#your-data)

## Install and update

**macOS and Linux**, in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.ps1 | iex
```

Nothing needs admin rights. You can also download the files yourself from the [latest release](https://github.com/stanleykinkelaar/Quiet/releases/latest).

Quiet updates itself. A few times a day it asks GitHub for the newest release. When there is one, it waits until its window is closed, then downloads it, checks it against the release's checksums, installs it and starts again in the tray. Switch off Update automatically in Settings to stop that; Check for updates in Settings still works then. Running the install command again works too.

What goes where:

| System | App | Your tasks |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\Quiet` | `%APPDATA%\Quiet\quiet.db` |
| macOS | `/Applications/Quiet.app` | `~/Library/Application Support/Quiet/quiet.db` |
| Linux | `~/.local/bin/quiet`, plus a launcher entry and icon | `~/.local/share/quiet/quiet.db` |

To uninstall on Windows, use Settings, Apps. On macOS or Linux, run the install command with `-s -- --uninstall` after `sh`. Your tasks stay either way.

### macOS

Quiet lives in the menu bar. It works on Apple Silicon and Intel Macs, macOS 12 or newer.

The app is not notarized by Apple (that needs a paid developer account). The install script downloads with `curl`, which macOS does not quarantine, so it opens normally. If you download `Quiet-macos.zip` from the browser instead, right click Quiet.app and choose Open the first time.

### Linux and Omarchy

On Linux your desktop owns global shortcuts, so Quiet cannot grab Ctrl+Alt+Space itself. Bind a key to `quiet --capture` instead. On Omarchy (Hyprland), add this line to `~/.config/hypr/bindings.conf`:

```text
bindd = CTRL ALT, SPACE, Quiet capture, exec, ~/.local/bin/quiet --capture
```

On Hyprland and other wlroots desktops, quick capture appears as a centred overlay, like a launcher, so it needs no window rules. The tray icon shows in Waybar's tray (Omarchy has one). Reminders arrive through your notification daemon (mako on Omarchy).

The Linux build is x86_64 only for now. It needs Wayland or X11 with Vulkan drivers, which Omarchy and most desktops already have.

## The four screens

* **Today** (1) is what you picked for today. The first task is marked Now.
* **Inbox** (2) is everything you captured and have not decided about yet.
* **Projects** (3) is a flat list of projects. Each keeps its own tasks, and one of them can be its next action.
* **Done** (4) shows what you finished, day by day. No totals, nothing to live up to.

Closing the window keeps Quiet in the tray (the menu bar on macOS). Use the tray menu's Quit, or Ctrl+Q, to really quit.

### Quick capture

Press Ctrl+Alt+Space anywhere. A small window with one text field appears. Type, press Enter, and the task lands in your Inbox and the window closes. Esc closes it without saving. You can change the shortcut in Settings (on Linux, see above).

## How the day works

* Today is yours to fill. Past five tasks you get a gentle note, nothing more.
* The first task in Today is marked Now, so starting needs no decision. Finish it and the next one moves up. Reorder with Alt+Up and Alt+Down, or by dragging, to change what comes first.
* Unfinished Today tasks stay overnight, under "From earlier". Keep them (K, or Keep all) or send them back to their project or the Inbox (T, or Send all back).
* Skipping a morning loses nothing. Nothing turns red and nothing is counted, apart from "done today".

## Planning your day

Press P on Today, or click the line at the top of Today until you have planned. Quiet walks you through it one question at a time:

1. **From earlier.** Each task left from an earlier day: keep it today or not.
2. **What you could do.** First tasks with a day in the coming week. Then one task from each project: its next action, or without one the task that has waited longest. Then the Inbox. For each: today, not today, or already done.
3. **What to start with.** If you picked two or more, you choose which one comes first.

At five tasks Plan stops and says the day is full. S stops it sooner. Closing it halfway is fine; what you decided stays decided.

## Sorting the Inbox

Press S on Today or the Inbox. Quiet shows Inbox tasks one at a time, and one key decides: Today, keep in Inbox, a project (1 to 9), already done, or delete. Deciding about a single thing is easier than facing a whole list. When ten or more tasks are waiting, Today offers it in one quiet line.

## Projects

Projects are flat: no sub projects, no sections. Press N on the Projects screen for a new one, Enter to open it, E to change its name or colour or to archive it. Archiving hides a project and takes its tasks out of Today. Nothing is deleted, and Show archived brings it back.

Inside a project, press A to make a task the project's **next action**. A project has at most one. Plan today offers it first.

### Project tasks in the Inbox

Moving a task into a project should not make it disappear from view. Press F on the Inbox (or click Projects) to show the tasks of all active projects below your Inbox tasks, each with its project's coloured tag. T picks one for today and A makes it the next action, without opening the project. Quiet remembers the choice. Sort still only asks about real Inbox tasks.

## Days, repeats and reminders

Open a task with Enter to give it any of these.

* **A day.** Type it the way you would say it: `fri`, `tomorrow`, `+3`, `2w` or `14 nov`. The task comes into Today on that day, or up to a week before if you choose. Sent back, it does not keep returning. After the day it simply stays, never red.
* **Repeat.** Every day, every weekday, or weekly on one day. The task comes into Today on its days. Completing it marks that day done; it comes back next time.
* **A reminder** is a time of day (`9`, `930`, `14:00` or `2pm`). It rings on days the task is in Today, so setting one puts the task in Today. The notification has In 10 min, In 1 hour and Done buttons. If nobody reacts, it rings once more 10 minutes later. A reminder you missed (the computer was off, or you ignored it) shows as a marked bell on the task until you click it or finish the task.

## All keys

| Key | What it does |
| --- | --- |
| Ctrl+Alt+Space | Quick capture from anywhere (changeable in Settings) |
| 1, 2, 3, 4 | Today, Inbox, Projects, Done |
| Ctrl+, (Cmd+, on macOS) | Settings |
| Ctrl+Q (Cmd+Q on macOS) | Quit (closing the window only hides it) |
| N | New task (or new project on the Projects screen) |
| Enter | Edit the selected task (open the selected project) |
| Space | Complete, or undo a completion |
| T | Put the task in Today, or take it out |
| A | Make it the project's next action (in a project, or on the Inbox with project tasks shown) |
| E | Edit the project (name, colour, archive) |
| K | Keep a task from an earlier day in Today |
| P | Plan today (on Today) |
| S | Sort the Inbox (on Today and Inbox) |
| F | Show project tasks in the Inbox, or hide them again |
| Delete | Delete the task (an Undo note appears) |
| Up, Down | Move the selection |
| Alt+Up, Alt+Down | Reorder Today (dragging works too) |
| Esc | Leave a text field, or go back from a project |

## Settings

Open Settings with Ctrl+, (Cmd+, on macOS).

* **Quick capture shortcut.** Type a new one and press Save. On Linux, your desktop's keyboard settings handle this instead.
* **Start with Windows** (Open at login on macOS, Start when you log in on Linux). Quiet waits in the tray after you sign in.
* **Updates.** Update automatically, or Check for updates yourself.
* **Your data.** Export tasks, and Show backups.

## Your data

Your list is one SQLite file on your own computer (see [What goes where](#install-and-update)). There is no account and no cloud. The only thing Quiet asks the internet is whether a new version exists, and you can switch that off in Settings.

**Backups.** Before an update changes the database, Quiet copies it into a `backups` folder next to `quiet.db` (for example `~/.local/share/quiet/backups`). The last three copies stay. If the copy cannot be made, Quiet does not touch the database and writes why to `quiet.log` next to it.

**Restoring a backup.** Quit Quiet (tray menu, Quit). Rename the copy you want to `quiet.db` and put it in place of the current one. Start Quiet again.

**Going back to an older version.** An older Quiet cannot read a list saved by a newer one. It does not try: it shows a notification, does not start, and leaves your list exactly as it was. Run the install command again to get the newest version back. To really go back, restore a backup made before the update.

**Export.** Settings, Your data, Export tasks writes every task and project, done ones included, to a JSON file in your Downloads folder. It is for keeping your list somewhere else; Quiet does not read it back in.

**Trying things safely.** Set the environment variable `QUIET_DB` to another path, and Quiet uses that file instead of your real list.
