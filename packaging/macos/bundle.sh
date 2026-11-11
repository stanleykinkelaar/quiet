#!/bin/sh
# Builds Quiet.app around a (universal) binary and zips it.
# Runs on macOS: it needs iconutil, codesign and ditto.
# Usage: packaging/macos/bundle.sh <quiet binary> <icons dir> <version> <out.zip>
set -eu

binary=$1
icons=$2
version=$3
out=$4
here=$(cd "$(dirname "$0")" && pwd)

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
app="$stage/Quiet.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 "$binary" "$app/Contents/MacOS/quiet"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
iconutil -c icns "$icons/quiet.iconset" -o "$app/Contents/Resources/quiet.icns"

# Ad hoc signature. Not notarized: install.sh downloads with curl, which
# does not quarantine, so Gatekeeper does not get in the way.
codesign --force --deep --sign - "$app"

mkdir -p "$(dirname "$out")"
ditto -c -k --keepParent "$app" "$out"
echo "wrote $out"
