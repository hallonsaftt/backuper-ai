#!/usr/bin/env bash
# E2E test na pravom sistemu (macOS, Windows/Git Bash, Linux), na LAŽNOM home folderu:
# backup → provera → vraćanje → istorija projekta → čuvanje istorije → zakazivanje.
# Proverava i da backup NE MENJA izvorne fajlove (SHA-256 + vreme + dozvole pre i posle).
#   tests/e2e.sh <putanja do claude-backup binarke> [python skripta]
set -euo pipefail
A="$1"
PY="${2:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) WIN=1 ;; *) WIN=0 ;; esac
if [ "$WIN" = 1 ]; then PYBIN=python; else PYBIN=python3; fi
export PYTHONUTF8=1 PYTHONIOENCODING=utf-8 # imena fajlova sa č/ć/š i na Windowsu
# putanja kako je vidi program (na Windowsu C:\…)
wp() { if [ "$WIN" = 1 ]; then cygpath -w "$1"; else printf '%s' "$1"; fi; }

W="$(mktemp -d)"
H="$W/home"
"$PYBIN" "$(wp "$HERE/make_fixture.py")" "$(wp "$H")" > /dev/null
mkdir -p "$W/out" "$W/out2" "$W/pyout" "$W/auto"
if [ "$WIN" = 1 ]; then
  # fixture ima Mac raspored; na Windowsu ga pokažemo preko istih promenljivih koje Claude koristi
  export USERPROFILE="$(wp "$H")" APPDATA="$(wp "$H/Library/Application Support")" \
    LOCALAPPDATA="$(wp "$H/AppData/Local")" CLAUDE_CODE_TMPDIR="$(wp "$H/tmp")"
else
  export HOME="$H" CLAUDE_CODE_TMPDIR="$H/tmp"
fi
# jezik programa (en podrazumevano, sr) — isto kao da je izabran u aplikaciji
CB_LANG="${CB_LANG:-en}"
mkdir -p "$H/Library/Application Support/ClaudeBackupTool"
printf '{"lang": "%s"}\n' "$CB_LANG" > "$H/Library/Application Support/ClaudeBackupTool/prefs.json"
if [ "$CB_LANG" = sr ]; then PD=claude-istorija; README=PROCITAJ_ME.txt; else PD=claude-history; README=README.txt; fi
echo "jezik: $CB_LANG"
ok() { printf '  \033[32m✔\033[0m %s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }

snap() {
  "$PYBIN" - "$(wp "$H")" <<'PY'
import hashlib, os, sys
root = sys.argv[1]
for d, ds, fs in os.walk(root):
    ds.sort()
    if "claude-istorija" in d.split(os.sep) or "claude-history" in d.split(os.sep) or "ClaudeBackupTool" in d.split(os.sep):
        continue
    for f in sorted(fs):
        p = os.path.join(d, f)
        r = os.path.relpath(p, root).replace(os.sep, "/")
        if os.path.islink(p):
            print("L", r, os.readlink(p))
            continue
        st = os.stat(p)
        h = hashlib.sha256(open(p, "rb").read()).hexdigest()
        print(h, st.st_mtime_ns, oct(st.st_mode), r)
PY
}
# provera zip-a nezavisno od programa: SHA256SUMS.txt naspram stvarnog sadržaja, i da nema login tokena
zipcheck() {
  "$PYBIN" - "$(wp "$1")" <<'PY'
import hashlib, sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
names = z.namelist()
assert not any("credentials" in n for n in names), "login token je u backupu"
root = names[0].split("/")[0]
sums = z.read(root + "/SHA256SUMS.txt").decode().splitlines()
for line in sums:
    h, arc = line.split("  ", 1)
    assert hashlib.sha256(z.read(root + "/" + arc)).hexdigest() == h, arc
print(f"  {len(sums)} kontrolnih zbirova se slaže")
PY
}
snap | sort > "$W/s0"

step "1. kompletan backup (zip)"
"$A" --backup --dest "$(wp "$W/out")" | tail -n 12
Z="$(ls "$W"/out/ClaudeBackup_*.zip)"
snap | sort > "$W/s1"
diff "$W/s0" "$W/s1" && ok "izvorni fajlovi nisu dirani (sadržaj, vreme, dozvole)"

step "2. provera backupa"
"$A" --verify "$(wp "$Z")" | tail -n 3
zipcheck "$Z" && ok "nezavisna provera (Python zipfile + SHA256SUMS.txt), bez login tokena"
"$PYBIN" - "$(wp "$Z")" "$README" <<'PY'
import sys, zipfile
names = zipfile.ZipFile(sys.argv[1]).namelist()
assert any(n.endswith("/" + sys.argv[2]) for n in names), f"nema {sys.argv[2]}"
PY
ok "fajlovi za čitanje su na izabranom jeziku ($README)"

step "3. backup kao folder, bez čitljivih formata"
"$A" --backup --folder --format none --dest "$(wp "$W/out2")" | tail -n 3
"$A" --verify "$(wp "$(ls -d "$W"/out2/ClaudeBackup_*)")" | tail -n 1

step "4. pokvaren backup mora da padne na proveri"
"$PYBIN" - "$(wp "$Z")" "$(wp "$W/bad.zip")" <<'PY'
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
if "$A" --verify "$(wp "$W/bad.zip")" > "$W/bad.txt"; then echo "✖ pokvaren backup je prošao proveru"; exit 1; fi
ok "pokvaren backup je odbijen"

step "5. vraćanje: obriši jedan razgovor, pa ga vrati"
T="$(ls "$H"/.claude/projects/*/cccc3333-*.jsonl)"
mv "$T" "$W/removed.jsonl"
"$A" --restore "$(wp "$Z")" | tail -n 4
test ! -e "$T" && ok "bez --yes ništa nije vraćeno"
"$A" --restore "$(wp "$Z")" --yes | tail -n 3
cmp "$T" "$W/removed.jsonl" && ok "vraćen fajl je bajt po bajt isti"
snap | sort > "$W/s2"
diff "$W/s0" "$W/s2" && ok "posle vraćanja sve je isto kao pre (i datumi izmene)"
"$A" --restore "$(wp "$Z")" --yes | tail -n 2
snap | sort > "$W/s3"
diff "$W/s0" "$W/s3" && ok "ponovno vraćanje ne menja ništa"

if [ -n "$PY" ]; then
  step "6. kompatibilnost sa Python skriptom"
  "$PYBIN" "$PY" --verify "$Z" | tail -n 3
  "$PYBIN" "$PY" --backup --all --format none --dest "$W/pyout" > /dev/null
  "$A" --verify "$(ls "$W"/pyout/ClaudeBackup_*.zip)" | tail -n 2
  ok "aplikacija i skripta čitaju backupe jedna druge"
fi

step "7. istorija jednog projekta"
"$A" --project-backup "$(wp "$H/www/POSCafe")" | tail -n 4
test -f "$H/www/POSCafe/$PD/.gitignore" && grep -qx '\*' "$H/www/POSCafe/$PD/.gitignore" && ok "$PD/.gitignore sa * postoji"
"$A" --verify "$(wp "$(ls -d "$H"/www/POSCafe/$PD/ClaudeBackup_*)")" | tail -n 1

step "8. čuvanje istorije (cleanupPeriodDays)"
"$A" --set-retention 3650 | tail -n 4
"$PYBIN" - "$(wp "$H/.claude/settings.json")" <<'PY'
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
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

if [ "$WIN" = 1 ]; then
  step "9. Task Scheduler zakazivanje"
  TASK='ClaudeBackup\Auto backup'
  "$A" --schedule weekly --at 14:00 --keep 3 --dest "$(wp "$W/auto")"
  MSYS_NO_PATHCONV=1 schtasks /Query /TN "$TASK" /V /FO LIST | grep -E "TaskName|Next Run Time|Status|Task To Run|Schedule Type" || true
  MSYS_NO_PATHCONV=1 schtasks /Query /TN "$TASK" > /dev/null && ok "Windows je prihvatio zadatak"
  test -f "$H/Library/Application Support/ClaudeBackupTool/schedule.json" && ok "podešavanje zapamćeno"
  "$A" --list | tail -n 2
  "$A" --unschedule
  if MSYS_NO_PATHCONV=1 schtasks /Query /TN "$TASK" > /dev/null 2>&1; then echo "✖ zadatak je ostao"; exit 1; fi
  ok "isključivanje uklanja zadatak"
fi

printf '\n\033[32mSVE PROVERE SU PROŠLE\033[0m\n'
