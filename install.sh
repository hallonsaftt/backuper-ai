#!/bin/bash
# Claude Backup — instalacija i ažuriranje jednom komandom (macOS, Apple Silicon):
#
#   curl -fsSL https://raw.githubusercontent.com/hallonsaftt/backuper-ai/main/install.sh | bash
#
# Fajl skinut preko curl-a macOS ne označava kao „preuzet sa interneta”, pa se aplikacija
# otvara odmah — bez „Open Anyway” i bez xattr komande. Isto radi i za novu verziju.
set -euo pipefail

APP="Claude Backup.app"
URL="https://github.com/hallonsaftt/backuper-ai/releases/latest/download/Claude-Backup-AppleSilicon.dmg"

say() { printf '\033[1m%s\033[0m\n' "$*"; }

[ "$(uname -s)" = "Darwin" ] || { echo "Ovo je instaler za macOS. Za Windows skini Claude-Backup-Windows-Setup.exe sa GitHub Releases."; exit 1; }
[ "$(uname -m)" = "arm64" ] || { echo "Ovaj build je za Mac sa Apple Silicon čipom (M1–M4)."; exit 1; }

DEST="${CB_DEST:-/Applications}"   # CB_DEST / CB_NO_OPEN služe za testiranje
if [ ! -w "$DEST" ]; then
  DEST="$HOME/Applications"
  mkdir -p "$DEST"
fi

WORK="$(mktemp -d)"
MNT="$WORK/mnt"
cleanup() {
  hdiutil detach "$MNT" -quiet 2>/dev/null || true
  rm -f "$WORK/cb.dmg"
  rmdir "$MNT" "$WORK" 2>/dev/null || true
}
trap cleanup EXIT

say "→ Skidam najnoviju verziju…"
curl -fL --progress-bar -o "$WORK/cb.dmg" "$URL"

say "→ Otvaram .dmg…"
mkdir -p "$MNT"
hdiutil attach "$WORK/cb.dmg" -nobrowse -readonly -mountpoint "$MNT" -quiet
[ -d "$MNT/$APP" ] || { echo "U .dmg-u nema $APP"; exit 1; }

# ako je aplikacija otvorena, zatvori je (automatski backup u pozadini nastavlja normalno)
osascript -e 'if application "Claude Backup" is running then tell application "Claude Backup" to quit' >/dev/null 2>&1 || true

say "→ Instaliram u $DEST…"
if [ -d "$DEST/$APP" ]; then
  # stara verzija ide u Otpad, ne briše se
  mv "$DEST/$APP" "$HOME/.Trash/Claude Backup (stara $(date +%Y%m%d-%H%M%S)).app" 2>/dev/null \
    || mv "$DEST/$APP" "$WORK/stara-verzija.app"
fi
ditto "$MNT/$APP" "$DEST/$APP"
xattr -dr com.apple.quarantine "$DEST/$APP" 2>/dev/null || true

say "✔ Instalirano: $DEST/$APP"
[ -n "${CB_NO_OPEN:-}" ] || open "$DEST/$APP"
