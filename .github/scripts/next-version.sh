#!/bin/sh
# Decides the next release from the conventional commits since the last v* tag.
#
#   .github/scripts/next-version.sh <notes file>
#
# Prints `release=true|false` and `version=x.y.z` (for $GITHUB_OUTPUT) and
# writes Markdown release notes to <notes file>.
#
# Bumps, following semver:
#   fix:, perf:                      patch  0.1.0 -> 0.1.1
#   feat:                            minor  0.1.0 -> 0.2.0
#   feat!:, BREAKING CHANGE: footer  major  1.2.0 -> 2.0.0
#   anything else (chore, docs, ci)  no release
# Before 1.0.0 a breaking change bumps the minor version instead, as is
# usual for 0.x. To jump to 1.0.0 (or any higher version), set it in
# Cargo.toml yourself; a Cargo.toml version above the last tag wins.
set -eu

notes=$1
cargo_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
last_tag=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)

if [ -n "$last_tag" ]; then range="$last_tag..HEAD"; else range=HEAD; fi

level=0 # 0 none, 1 patch, 2 minor, 3 major
breaking=""
features=""
fixes=""
for sha in $(git log --format=%H "$range"); do
    subject=$(git log -1 --format=%s "$sha")
    body=$(git log -1 --format=%b "$sha")
    short=$(git log -1 --format=%h "$sha")
    scope=$(printf '%s\n' "$subject" | sed -n 's/^[a-z]*(\([^)]*\)).*/\1/p')
    line="* ${scope:+**$scope:** }${subject#*: } ($short)"
    case $subject in
        "chore(release)"*) continue ;;
    esac
    if printf '%s\n' "$subject" | grep -Eq '^[a-z]+(\([^)]*\))?!:' ||
        printf '%s\n' "$body" | grep -Eq '^BREAKING[ -]CHANGE:'; then
        level=3
        breaking="$breaking$line
"
    fi
    if printf '%s\n' "$subject" | grep -Eq '^feat(\([^)]*\))?!?:'; then
        [ $level -lt 2 ] && level=2
        features="$features$line
"
    elif printf '%s\n' "$subject" | grep -Eq '^(fix|perf)(\([^)]*\))?!?:'; then
        [ $level -lt 1 ] && level=1
        fixes="$fixes$line
"
    fi
done

if [ -z "$last_tag" ]; then
    # First release: ship whatever Cargo.toml says.
    next=$cargo_version
elif [ $level -eq 0 ]; then
    next=""
else
    base=${last_tag#v}
    major=$(echo "$base" | cut -d. -f1)
    minor=$(echo "$base" | cut -d. -f2)
    patch=$(echo "$base" | cut -d. -f3 | cut -d- -f1)
    if [ $level -eq 3 ] && [ "$major" -eq 0 ]; then level=2; fi
    case $level in
        3) next="$((major + 1)).0.0" ;;
        2) next="$major.$((minor + 1)).0" ;;
        1) next="$major.$minor.$((patch + 1))" ;;
    esac
    # A hand-set higher version in Cargo.toml wins.
    highest=$(printf '%s\n%s\n' "$next" "$cargo_version" | sort -V | tail -n 1)
    next=$highest
fi

if [ -z "$next" ]; then
    echo "release=false"
    echo "version=$cargo_version"
    exit 0
fi

{
    if [ -n "$breaking" ]; then printf '## Breaking changes\n\n%s\n' "$breaking"; fi
    if [ -n "$features" ]; then printf '## Features\n\n%s\n' "$features"; fi
    if [ -n "$fixes" ]; then printf '## Fixes\n\n%s\n' "$fixes"; fi
    printf '## Install\n\n'
    printf 'macOS and Linux: `curl -fsSL https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.sh | sh`\n\n'
    printf 'Windows (PowerShell): `irm https://raw.githubusercontent.com/stanleykinkelaar/Quiet/main/install.ps1 | iex`\n'
} > "$notes"

echo "release=true"
echo "version=$next"
