#!/bin/sh
# Packs the Linux build into a tarball that install.sh unpacks:
#   quiet/quiet, quiet/quiet.desktop, quiet/icons/hicolor/...
# Usage: packaging/linux/package.sh <quiet binary> <icons dir> <out.tar.gz>
set -eu

binary=$1
icons=$2
out=$3

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/quiet/icons"
install -m 755 "$binary" "$stage/quiet/quiet"
install -m 644 "$(dirname "$0")/quiet.desktop" "$stage/quiet/quiet.desktop"
cp -R "$icons/hicolor" "$stage/quiet/icons/hicolor"
mkdir -p "$(dirname "$out")"
tar -C "$stage" -czf "$out" quiet
echo "wrote $out"
