#!/usr/bin/env bash
# E2E test na pravom sistemu, na LAŽNOM home folderu:
# backup → provera → vraćanje → istorija projekta → čuvanje istorije → (macOS) launchd.
# Proverava i da backup NE MENJA izvorne fajlove (SHA-256 + vreme + dozvole pre i posle).
#   tests/e2e.sh <putanja do claude-backup binarke> [python skripta]
set -euo pipefail
A="$1"
PY="${2:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
W="$(mktemp -d)"
H="$W/home"
python3 "$HERE/make_fixture.py" "$H" >/dev/null
export HOME="$H" CLAUDE_CODE_TMPDIR="$H/tmp"
mkdir -p "$W/out" "$W/out2" "$W/pyout" "$W/auto"
ok() { printf '  \033[32m✔\033[0m %s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }

snap() {
  python3 - "$H" <<'PY'
import hashlib, os, sys
root = sys.argv[1]
for d, ds, fs in os.walk(root):
    ds.sort()
    if "claude-istorija" in d.split(os.sep):
        continue
    for f in sorted(fs):
        p = os.path.join(d, f)
        r = os.path.relpath(p, root)
        if os.path.islink(p):
            print("L", r, os.readlink(p))
            continue
        st = os.stat(p)
        h = hashlib.sha256(open(p, "rb").read()).hexdigest()
        print(h, st.st_mtime_ns, oct(st.st_mode), r)
PY
}
snap | sort > "$W/s0"

step "1. kompletan backup (zip)"
"$A" --backup --dest "$W/out" | tail -n 12
Z="$(ls "$W"/out/ClaudeBackup_*.zip)"
snap | sort > "$W/s1"
diff "$W/s0" "$W/s1" && ok "izvorni fajlovi nisu dirani (sadržaj, vreme, dozvole)"
if unzip -l "$Z" | grep -q "credentials"; then echo "✖ login token je u backupu"; exit 1; fi
ok "login token (.credentials.json) nije u backupu"

step "2. provera backupa"
"$A" --verify "$Z" | tail -n 3
(cd "$W" && rm -rf unz && mkdir unz && cd unz && unzip -q "$Z" && cd ClaudeBackup_* && shasum -a 256 -c SHA256SUMS.txt >/dev/null) && ok "nezavisna provera: unzip + shasum -c SHA256SUMS.txt"

step "3. backup kao folder, bez čitljivih formata"
"$A" --backup --folder --format none --dest "$W/out2" | tail -n 3
"$A" --verify "$(ls -d "$W"/out2/ClaudeBackup_*)" | tail -n 1

step "4. pokvaren backup mora da padne na proveri"
python3 - "$Z" "$W/bad.zip" <<'PY'
import sys, zipfile
src, dst = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(src) as a, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as b:
    done = False
    for i in a.infolist():
        data = a.read(i.filename)
        if not done and i.filename.endswith(".jsonl") and "/original/" in "/" + i.filename:
            data = data.replace(b"a", b"b", 1); done = True
        b.writestr(i, data)
PY
if "$A" --verify "$W/bad.zip" > "$W/bad.txt"; then echo "✖ pokvaren backup je prošao proveru"; exit 1; fi
ok "pokvaren backup je odbijen"

step "5. vraćanje: obriši jedan razgovor, pa ga vrati"
T="$(ls "$H"/.claude/projects/*/cccc3333-*.jsonl)"
mv "$T" "$W/removed.jsonl"
"$A" --restore "$Z" | tail -n 4
test ! -e "$T" && ok "bez --yes ništa nije vraćeno"
"$A" --restore "$Z" --yes | tail -n 3
cmp "$T" "$W/removed.jsonl" && ok "vraćen fajl je bajt po bajt isti"
snap | sort > "$W/s2"
diff "$W/s0" "$W/s2" && ok "posle vraćanja sve je isto kao pre (i datumi izmene)"
"$A" --restore "$Z" --yes | tail -n 2
snap | sort > "$W/s3"
diff "$W/s0" "$W/s3" && ok "ponovno vraćanje ne menja ništa"

if [ -n "$PY" ]; then
  step "6. kompatibilnost sa Python skriptom"
  python3 "$PY" --verify "$Z" | tail -n 3
  python3 "$PY" --backup --all --format none --dest "$W/pyout" > /dev/null
  "$A" --verify "$(ls "$W"/pyout/ClaudeBackup_*.zip)" | tail -n 2
  ok "aplikacija i skripta čitaju backupe jedna druge"
fi

step "7. istorija jednog projekta"
"$A" --project-backup "$H/www/POSCafe" | tail -n 4
test -f "$H/www/POSCafe/claude-istorija/.gitignore" && grep -qx '\*' "$H/www/POSCafe/claude-istorija/.gitignore" && ok ".gitignore sa * postoji"
"$A" --verify "$(ls -d "$H"/www/POSCafe/claude-istorija/ClaudeBackup_*)" | tail -n 1

step "8. čuvanje istorije (cleanupPeriodDays)"
"$A" --set-retention 3650 | tail -n 4
python3 - "$H/.claude/settings.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
assert d["cleanupPeriodDays"] == 3650, d
assert d["model"] == "opus" and d["env"] == {"FOO": "1"}, d
PY
ls "$H"/.claude/settings.json.pre-claude-backup-* > /dev/null && ok "settings.json: samo novi ključ, ostalo isto; napravljena kopija starog"

if [ "$(uname)" = "Darwin" ]; then
  step "9. launchd zakazivanje (informativno)"
  if "$A" --schedule daily --at 14:00 --keep 3 --dest "$W/auto"; then
    plutil -lint "$H/Library/LaunchAgents/com.claudebackup.auto.plist"
    launchctl print "gui/$(id -u)/com.claudebackup.auto" 2>&1 | grep -E "state|program|arguments" | head -n 5 || true
    "$A" --unschedule || true
  else
    echo "  (launchd nije dostupan na ovom runneru)"
  fi
fi

printf '\n\033[32mSVE PROVERE SU PROŠLE\033[0m\n'
