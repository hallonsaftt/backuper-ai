#!/bin/bash
# Claude Backup — instalacija i ažuriranje jednom komandom (macOS, Apple Silicon).
#
# Javni repo:      curl -fsSL https://raw.githubusercontent.com/hallonsaftt/backuper-ai/main/install.sh | bash
# Privatni repo:   gh api repos/hallonsaftt/backuper-ai/contents/install.sh -H "Accept: application/vnd.github.raw" | bash
# Već skinut .dmg: bash install.sh ~/Downloads/Claude-Backup-AppleSilicon.dmg
#
# Aplikacija se instalira bez „preuzeto sa interneta” oznake, pa se otvara odmah —
# bez „Open Anyway” i bez xattr komande. Isti red služi i za novu verziju.
set -euo pipefail

REPO="hallonsaftt/backuper-ai"
ASSET="Claude-Backup-AppleSilicon.dmg"
APP="Claude Backup.app"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
die() { printf '\033[31m✖ %s\033[0m\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "Ovo je instaler za macOS. Za Windows skini Claude-Backup-Windows-Setup.exe sa GitHub Releases."
[ "$(uname -m)" = "arm64" ] || die "Ovaj build je za Mac sa Apple Silicon čipom (M1–M4)."

DEST="${CB_DEST:-/Applications}" # CB_DEST / CB_NO_OPEN služe za testiranje
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
  [ -f "$1" ] || die "Nema fajla: $1"
  say "→ Koristim $1"
  cp "$1" "$DMG"
elif command -v gh > /dev/null 2>&1 && gh auth status > /dev/null 2>&1; then
  say "→ Skidam najnoviju verziju (GitHub CLI)…"
  gh release download --repo "$REPO" --pattern "$ASSET" --dir "$WORK" --clobber
else
  say "→ Skidam najnoviju verziju…"
  if ! curl -fL --progress-bar -o "$DMG" "https://github.com/$REPO/releases/latest/download/$ASSET"; then
    die "Ne mogu da skinem $ASSET. Ako je repo privatan: uloguj se sa „gh auth login” pa pokreni ponovo,
   ili skini .dmg u browseru i pokreni:  bash install.sh ~/Downloads/$ASSET"
  fi
fi

say "→ Otvaram .dmg…"
mkdir -p "$MNT"
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$MNT" -quiet
[ -d "$MNT/$APP" ] || die "U .dmg-u nema $APP"

# ako je aplikacija otvorena, zatvori je (automatski backup u pozadini nastavlja normalno)
osascript -e 'if application "Claude Backup" is running then tell application "Claude Backup" to quit' > /dev/null 2>&1 || true

say "→ Instaliram u ${DEST}…"
if [ -d "$DEST/$APP" ]; then
  # stara verzija ide u Otpad, ne briše se
  mv "$DEST/$APP" "$HOME/.Trash/Claude Backup (stara $(date +%Y%m%d-%H%M%S)).app" 2> /dev/null ||
    mv "$DEST/$APP" "$WORK/stara-verzija.app"
fi
ditto "$MNT/$APP" "$DEST/$APP"
xattr -dr com.apple.quarantine "$DEST/$APP" 2> /dev/null || true

say "✔ Instalirano: $DEST/$APP"
[ -n "${CB_NO_OPEN:-}" ] || open "$DEST/$APP"
