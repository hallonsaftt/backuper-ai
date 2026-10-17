#!/bin/bash
# Claude Backup — install and update with one command (macOS, Apple Silicon).
#
# Public repo:         curl -fsSL https://raw.githubusercontent.com/hallonsaftt/backuper-ai/main/install.sh | bash
# Private repo:        gh api repos/hallonsaftt/backuper-ai/contents/install.sh -H "Accept: application/vnd.github.raw" | bash
# Already have a .dmg: bash install.sh ~/Downloads/Claude-Backup-AppleSilicon.dmg
#
# The app is installed without the "downloaded from the internet" flag, so it opens right away —
# no "Open Anyway" and no xattr command. The same line installs a newer version.
# Messages are in English; with CB_LANG=sr (or a Serbian LANG) they're in Serbian.
set -euo pipefail

REPO="hallonsaftt/backuper-ai"
ASSET="Claude-Backup-AppleSilicon.dmg"
APP="Claude Backup.app"

case "${CB_LANG:-${LANG:-}}" in sr*) SR=1 ;; *) SR=0 ;; esac
m() { if [ "$SR" = 1 ]; then printf '%s' "$1"; else printf '%s' "$2"; fi; } # m "srpski" "english"
say() { printf '\033[1m%s\033[0m\n' "$*"; }
die() { printf '\033[31m✖ %s\033[0m\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "$(m "Ovo je instaler za macOS. Za Windows skini Claude-Backup-Windows-Setup.exe sa GitHub Releases." \
  "This installer is for macOS. On Windows, download Claude-Backup-Windows-Setup.exe from GitHub Releases.")"
[ "$(uname -m)" = "arm64" ] || die "$(m "Ovaj build je za Mac sa Apple Silicon čipom (M1–M4)." "This build is for Macs with an Apple Silicon chip (M1–M4).")"

DEST="${CB_DEST:-/Applications}" # CB_DEST / CB_NO_OPEN are for testing
if [ ! -w "$DEST" ]; then
  DEST="$HOME/Applications"
  mkdir -p "$DEST"
fi

WORK="$(mktemp -d)"
MNT="$WORK/mnt"
DMG="$WORK/$ASSET"
cleanup() {
  hdiutil detach "$MNT" -quiet 2>/dev/null || true
  rm -f "$WORK/$ASSET"
  rmdir "$MNT" "$WORK" 2>/dev/null || true
}
trap cleanup EXIT

if [ -n "${1:-}" ]; then
  [ -f "$1" ] || die "$(m "Nema fajla" "No such file"): $1"
  say "→ $(m "Koristim" "Using") $1"
  cp "$1" "$DMG"
elif command -v gh > /dev/null 2>&1 && gh auth status > /dev/null 2>&1; then
  say "→ $(m "Skidam najnoviju verziju" "Downloading the latest version") (GitHub CLI)…"
  gh release download --repo "$REPO" --pattern "$ASSET" --dir "$WORK" --clobber
else
  say "→ $(m "Skidam najnoviju verziju" "Downloading the latest version")…"
  if ! curl -fL --progress-bar -o "$DMG" "https://github.com/$REPO/releases/latest/download/$ASSET"; then
    die "$(m "Ne mogu da skinem $ASSET. Ako je repo privatan: uloguj se sa „gh auth login” pa pokreni ponovo,
   ili skini .dmg u browseru i pokreni:  bash install.sh ~/Downloads/$ASSET" \
      "Can't download $ASSET. If the repo is private: sign in with \"gh auth login\" and run this again,
   or download the .dmg in a browser and run:  bash install.sh ~/Downloads/$ASSET")"
  fi
fi

say "→ $(m "Otvaram .dmg" "Opening the .dmg")…"
mkdir -p "$MNT"
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$MNT" -quiet
[ -d "$MNT/$APP" ] || die "$(m "U .dmg-u nema" "The .dmg doesn't contain") $APP"

# if the app is open, quit it (automatic backup in the background carries on normally)
osascript -e 'if application "Claude Backup" is running then tell application "Claude Backup" to quit' > /dev/null 2>&1 || true

say "→ $(m "Instaliram u" "Installing to") ${DEST}…"
if [ -d "$DEST/$APP" ]; then
  # the old version goes to the Trash, it isn't deleted
  mv "$DEST/$APP" "$HOME/.Trash/Claude Backup ($(m stara old) $(date +%Y%m%d-%H%M%S)).app" 2> /dev/null ||
    mv "$DEST/$APP" "$WORK/old-version.app"
fi
ditto "$MNT/$APP" "$DEST/$APP"
xattr -dr com.apple.quarantine "$DEST/$APP" 2> /dev/null || true

say "✔ $(m "Instalirano" "Installed"): $DEST/$APP"
[ -n "${CB_NO_OPEN:-}" ] || open "$DEST/$APP"
