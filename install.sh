#!/bin/sh
# Installs (or updates) Quiet on macOS and Linux from the latest GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh
#
# Uninstall:
#
#   curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh -s -- --uninstall
#
# No root needed. Your tasks are never touched.
set -eu

REPO="stanleykinkelaar/Quiet"
# QUIET_INSTALL_BASE points at another download location, for testing.
BASE="${QUIET_INSTALL_BASE:-https://github.com/$REPO/releases/latest/download}"

say() { printf '%s\n' "$*"; }
fail() { printf 'quiet: %s\n' "$*" >&2; exit 1; }

uninstall=0
for arg in "$@"; do
    case $arg in
        --uninstall) uninstall=1 ;;
        *) fail "unknown option $arg" ;;
    esac
done

command -v curl >/dev/null || fail "curl is required"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

stop_running() {
    pkill -x quiet 2>/dev/null && sleep 1 || true
}

install_macos() {
    if [ -w /Applications ]; then dest=/Applications; else dest="$HOME/Applications"; fi
    if [ $uninstall = 1 ]; then
        stop_running
        rm -rf "/Applications/Quiet.app" "$HOME/Applications/Quiet.app"
        rm -f "$HOME/Library/LaunchAgents/Quiet.plist"
        say "Quiet is removed. Your tasks are still in ~/Library/Application Support/Quiet."
        return
    fi
    say "Downloading Quiet for macOS..."
    curl -fL --progress-bar "$BASE/Quiet-macos.zip" -o "$tmp/Quiet.zip"
    ditto -x -k "$tmp/Quiet.zip" "$tmp"
    stop_running
    mkdir -p "$dest"
    rm -rf "$dest/Quiet.app"
    mv "$tmp/Quiet.app" "$dest/Quiet.app"
    # curl does not quarantine downloads, but be sure: the app is not notarized.
    xattr -dr com.apple.quarantine "$dest/Quiet.app" 2>/dev/null || true
    open "$dest/Quiet.app"
    say "Quiet is installed in $dest and running. Look for it in the menu bar."
}

install_linux() {
    bin="$HOME/.local/bin"
    data="${XDG_DATA_HOME:-$HOME/.local/share}"
    if [ $uninstall = 1 ]; then
        stop_running
        rm -f "$bin/quiet" "$data/applications/quiet.desktop" "$HOME/.config/autostart/Quiet.desktop"
        find "$data/icons/hicolor" -path '*/apps/quiet.png' -delete 2>/dev/null || true
        say "Quiet is removed. Your tasks are still in $data/quiet."
        return
    fi
    case $(uname -m) in
        x86_64 | amd64) ;;
        *) fail "only x86_64 Linux builds exist for now (this is $(uname -m))" ;;
    esac
    say "Downloading Quiet for Linux..."
    curl -fL --progress-bar "$BASE/quiet-linux-x86_64.tar.gz" -o "$tmp/quiet.tar.gz"
    tar -xzf "$tmp/quiet.tar.gz" -C "$tmp"

    mkdir -p "$bin" "$data/applications" "$data/icons"
    # Replace by rename, so a running copy is not disturbed mid-write.
    install -m 755 "$tmp/quiet/quiet" "$bin/.quiet.new"
    mv -f "$bin/.quiet.new" "$bin/quiet"
    cp -R "$tmp/quiet/icons/hicolor" "$data/icons/"
    sed "s|^Exec=quiet|Exec=$bin/quiet|" "$tmp/quiet/quiet.desktop" > "$data/applications/quiet.desktop"
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$data/applications" 2>/dev/null || true

    if pgrep -x quiet >/dev/null 2>&1; then
        say "Quiet is updated. Quit it from the tray and start it again to use the new version."
    else
        say "Quiet is installed. Start it from your app launcher, or run: quiet"
    fi
    case ":$PATH:" in
        *":$bin:"*) ;;
        *) say "Note: $bin is not on your PATH. Add it to run 'quiet' from a terminal." ;;
    esac

    say ""
    say "Quick capture needs a shortcut in your desktop that runs: $bin/quiet --capture"
    if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
        say "On Hyprland (Omarchy), add this line to ~/.config/hypr/bindings.conf:"
        say ""
        say "  bindd = CTRL ALT, SPACE, Quiet capture, exec, $bin/quiet --capture"
    fi
}

case $(uname -s) in
    Darwin) install_macos ;;
    Linux) install_linux ;;
    *) fail "this script is for macOS and Linux. On Windows, see the README." ;;
esac
