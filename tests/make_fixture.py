"""Pravi lazni macOS HOME sa realisticnim Claude Code / Desktop / Cowork podacima."""
import json, os, sys, uuid, base64, shutil
from pathlib import Path

H = Path(sys.argv[1])
if H.exists():
    shutil.rmtree(H)
H.mkdir(parents=True)
C = H / ".claude"
APP = H / "Library/Application Support/Claude"
(H / "Desktop").mkdir()
(H / "Documents").mkdir()
(H / "Downloads").mkdir()

def w(p, s, mode=None):
    p = Path(p); p.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(s, bytes): p.write_bytes(s)
    else: p.write_text(s, encoding="utf-8")
    if mode: os.chmod(p, mode)
    return p

def enc(p): import re; return re.sub(r"[^A-Za-z0-9]", "-", p)

PNG = base64.b64encode(bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000100000001008020000009091683600000017494441547801"
    "63fccf8001189818a8061a6a20a81a00e43a0b0d2f1ab8a50000000049454e44ae426082")).decode()

def tl(path, entries):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        for e in entries:
            f.write((e if isinstance(e, str) else json.dumps(e, ensure_ascii=False)) + "\n")

class T:
    def __init__(self, sid, cwd, entry="cli"):
        self.sid, self.cwd, self.entry = sid, cwd, entry
        self.rows, self.t = [], 0
    def ts(self):
        self.t += 1
        return f"2026-09-0{1 + self.t // 50}T10:{(self.t % 50):02d}:00.123Z"
    def add(self, typ, msg=None, parent=None, **kw):
        u = str(uuid.uuid4())
        e = {"parentUuid": parent, "isSidechain": False, "userType": "external", "cwd": self.cwd, "sessionId": self.sid,
             "version": "2.1.250", "entrypoint": self.entry, "gitBranch": "main", "type": typ, "uuid": u, "timestamp": self.ts()}
        if msg is not None: e["message"] = msg
        e.update(kw)
        self.rows.append(e)
        return u
    def user(self, content, parent, **kw): return self.add("user", {"role": "user", "content": content}, parent, **kw)
    def asst(self, blocks, parent, model="claude-opus-4-5", **kw):
        return self.add("assistant", {"model": model, "id": "msg_" + uuid.uuid4().hex[:10], "type": "message",
                                      "role": "assistant", "content": blocks}, parent, **kw)

# ---------------- projekat POSCafe (na disku)
POS = H / "www/POSCafe"
w(POS / "app/Http/Controllers/CartController.php", "<?php // KOD - ne sme u backup\n")
w(POS / "CLAUDE.md", "# POSCafe\nLaravel 11, MySQL. Uvek pokreni testove.\n")
w(POS / "CLAUDE.local.md", "Moj lokalni kontekst\n")
w(POS / ".mcp.json", '{"mcpServers":{"db":{"command":"mysql-mcp"}}}\n')
w(POS / ".claude/settings.local.json", '{"permissions":{"allow":["Bash(php artisan test)"]}}\n')
w(POS / ".claude/commands/deploy.md", "Deploy na Hostinger\n")
w(POS / ".claude/worktrees/feature-x/app.php", "<?php // worktree KOD\n")
w(POS / "app/Http/CLAUDE.md", "Kontroleri: tanki, logika u servisima.\n")
w(POS / "node_modules/pkg/CLAUDE.md", "tuđi paket - preskoci\n")

t = T("aaaa1111-0000-4000-8000-000000000001", str(POS))
u1 = t.user("<system-reminder>skriveni harness tekst</system-reminder>Zašto korpa ne računa PDV? 🧾", None)
a1 = t.asst([{"type": "thinking", "thinking": "Treba pogledati CartController.", "signature": "x"}], u1)
a1b = t.asst([{"type": "text", "text": "Pogledaću `CartController`."},
              {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "grep -n vat app/Http/Controllers/CartController.php", "description": "Traži PDV"}}], a1)
r1 = t.user([{"type": "tool_result", "tool_use_id": "toolu_1", "content": "42:    $vat = 0; // TODO"}], a1b, toolUseResult={"stdout": "42"})
a2 = t.asst([{"type": "text", "text": "## Uzrok\n\nNa liniji **42** je `$vat = 0`.\n\n| Fajl | Linija |\n|---|---|\n| CartController.php | 42 |\n\n```php\n$vat = $subtotal * 0.2;\n```\n\nVidi [Laravel docs](https://laravel.com/docs)."}], r1)
# grana koja je kasnije izmenjena (rewind) - NE treba da se vidi
u_old = t.user("Popravi to (STARA VERZIJA PORUKE)", a2)
a_old = t.asst([{"type": "text", "text": "STARI ODGOVOR koji je odbačen"}], u_old)
# nova verzija iste poruke (isti parent a2)
u_new = t.user("Popravi to i dodaj test", a2)
a3 = t.asst([{"type": "tool_use", "id": "toolu_2", "name": "Edit", "input": {"file_path": str(POS / "app/Http/Controllers/CartController.php"), "old_string": "$vat = 0;", "new_string": "$vat = $subtotal * 0.2;"}}], u_new)
r2 = t.user([{"type": "tool_result", "tool_use_id": "toolu_2", "content": "Permission denied", "is_error": True}], a3)
a4 = t.asst([{"type": "text", "text": "Izmena nije uspela zbog dozvola."}], r2)
# sidechain (subagent stari format) - ne treba u glavnom prikazu
t.add("user", {"role": "user", "content": "SUBAGENT PROMPT"}, a4, isSidechain=True)
meta = t.user("Caveat: META PORUKA", a4, isMeta=True)
cmd = t.user("<command-name>/model</command-name>\n<command-message>model</command-message>\n<command-args>opus</command-args>", meta)
out = t.user("<local-command-stdout>Set model to opus</local-command-stdout>", cmd)
bash = t.user("<bash-input>php artisan test</bash-input>", out)
bout = t.user("<bash-stdout>Tests: 12 passed</bash-stdout><bash-stderr></bash-stderr>", bash)
img = t.user([{"type": "text", "text": "Evo screenshot-a greške"}, {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG}}], bout)
a5 = t.asst([{"type": "text", "text": "Vidim grešku na slici."}], img)
intr = t.user([{"type": "text", "text": "[Request interrupted by user]"}], a5)
# compact
cb = t.add("system", None, None, subtype="compact_boundary", content="Conversation compacted", logicalParentUuid=intr,
           compactMetadata={"trigger": "auto", "preTokens": 150000}, level="info")
cs = t.user("This session is being continued from a previous conversation. Summary: popravljen PDV.", cb, isCompactSummary=True)
u_after = t.user("Nastavi sa testovima posle compact-a", cs)
big = "X" * 70000
a6 = t.asst([{"type": "tool_use", "id": "toolu_3", "name": "Read", "input": {"file_path": "/tmp/big.log"}}], u_after)
r3 = t.user([{"type": "tool_result", "tool_use_id": "toolu_3", "content": [{"type": "text", "text": big}]}], a6)
a7 = t.asst([{"type": "text", "text": "API Error: 529 overloaded"}], r3, model="<synthetic>", isApiErrorMessage=True)
a8 = t.asst([{"type": "text", "text": "Gotovo — sve radi. ```backticks``` u tekstu."}], a7)
t.rows.append({"type": "summary", "summary": "Popravka PDV-a u korpi", "leafUuid": a8})
pos_dir = C / "projects" / enc(str(POS))
tl(pos_dir / f"{t.sid}.jsonl", t.rows + ["{pokvaren red bez kraja"])
w(pos_dir / t.sid / "subagents/agent-a1.jsonl", json.dumps({"type": "user", "isSidechain": True, "message": {"role": "user", "content": "sub"}}) + "\n")
w(pos_dir / t.sid / "tool-results/toolu_9.txt", "veliki izlaz\n")
w(pos_dir / "memory/MEMORY.md", "- Korisnik voli kratke odgovore\n")
w(pos_dir / "aaaa1111-0000-4000-8000-000000000001.orphaned-1700000000-abc.jsonl", "{}\n")

# druga sesija u istom projektu - pokrenuta iz Desktop aplikacije (ima karticu)
t2 = T("bbbb2222-0000-4000-8000-000000000002", str(POS), entry="claude-desktop")
x1 = t2.user("Napravi migraciju za tabelu orders", None)
t2.asst([{"type": "text", "text": "Napravio sam migraciju."}], x1)
tl(pos_dir / f"{t2.sid}.jsonl", t2.rows)

# ---------------- projekat koji vise ne postoji na disku
gone = "/Users/petar/stari-projekat"
t3 = T("cccc3333-0000-4000-8000-000000000003", gone, entry="claude-vscode")
y1 = t3.user("Stari razgovor", None)
t3.asst([{"type": "text", "text": "Odgovor iz VS Code-a"}], y1)
tl(C / "projects" / enc(gone) / f"{t3.sid}.jsonl", t3.rows)
os.utime(C / "projects" / enc(gone) / f"{t3.sid}.jsonl", (1600000000, 1600000000))  # star 6 godina

# ---------------- sesija u HOME
t4 = T("dddd4444-0000-4000-8000-000000000004", str(H))
z1 = t4.user("Pitanje iz početnog foldera", None)
t4.asst([{"type": "text", "text": "Odgovor"}], z1)
tl(C / "projects" / enc(str(H)) / f"{t4.sid}.jsonl", t4.rows)

# ---------------- projekat samo u .claude.json (bez razgovora), sa CLAUDE.md
LAGER = H / "www/LagerSoft Ć"
w(LAGER / "CLAUDE.md", "# LagerSoft\n")
w(LAGER / "index.php", "<?php kod\n")

# ---------------- ~/.claude.json
w(H / ".claude.json", json.dumps({"numStartups": 42, "projects": {str(POS): {"allowedTools": []}, str(LAGER): {}, gone: {}, str(H): {}},
                                  "mcpServers": {"hostinger": {"command": "npx"}}, "oauthAccount": {"emailAddress": "x@y.z"}}, indent=2))
w(H / ".claude.json.backup", "{}")

# ---------------- ~/.claude ostalo
w(C / "settings.json", json.dumps({"model": "opus", "permissions": {"allow": ["Bash(ls)"]}, "env": {"FOO": "1"}}, indent=4), 0o600)
w(C / "CLAUDE.md", "# Globalna pravila\nOdgovaraj na srpskom.\n")
w(C / "skills/laravel-deploy/SKILL.md", "---\nname: laravel-deploy\n---\nKoraci...\n")
w(C / "skills/laravel-deploy/scripts/deploy.sh", "#!/bin/bash\necho deploy\n", 0o755)
w(C / "skills/Šifre i čćž/SKILL.md", "unicode ime\n")
w(H / "dev/shared-skill/SKILL.md", "deljeni skill van ~/.claude\n")
os.symlink(str(H / "dev/shared-skill"), C / "skills/shared-skill")
w(C / "agents/reviewer.md", "---\nname: reviewer\n---\n")
w(C / "commands/fix issue.md", "komanda sa razmakom\n")
w(C / "plugins/installed_plugins.json", "{}")
w(C / "history.jsonl", '{"display":"Zašto korpa","project":"/x"}\n')
w(C / "file-history" / t.sid / "abc@v1", "stari sadržaj fajla\n")
w(C / "plans/plan-1.md", "# Plan\n")
w(C / "todos/old.json", "[]")
(C / "rules").mkdir(parents=True)  # prazan folder
w(C / "statusline.sh", "#!/bin/sh\necho hi\n", 0o755)
w(C / "debug/latest.txt", "debug log")
w(C / "statsig/cache", "x")
w(C / "shell-snapshots/snap.sh", "x")
w(C / "local/node_modules/claude/cli.js", "binarni program")
w(C / ".credentials.json", '{"token":"TAJNA"}', 0o600)
w(C / "sessions/123.json", "{}")

# ---------------- Desktop aplikacija
w(APP / "claude-code-sessions/acc-1/org-1/local_aaa.json", json.dumps({"sessionId": "local_aaa", "cliSessionId": t2.sid,
  "cwd": str(POS), "title": "Migracija za orders (Desktop naslov)", "model": "opus", "isArchived": False}))
w(APP / "claude_desktop_config.json", '{"mcpServers":{"fs":{"command":"npx"}}}')
w(APP / "config.json", '{"oauth:tokenCache":"enc"}')
w(APP / "Cache/Cache_Data/f_0001", "kes")
w(APP / "vm_bundles/claudevm.bundle/rootfs.img", "x")
w(APP / "Claude Extensions/ext.fs/manifest.json", "{}")
w(APP / "Local Storage/leveldb/000003.log", "ui")
CW = APP / "local-agent-mode-sessions/acc-1/org-1"
t5 = T("eeee5555-0000-4000-8000-000000000005", "/sessions/vesela-ptica", entry="claude-desktop")
q1 = t5.user("Napravi izveštaj o prodaji za septembar", None)
q2 = t5.asst([{"type": "tool_use", "id": "toolu_c1", "name": "Write", "input": {"file_path": "/sessions/vesela-ptica/outputs/izvestaj.md", "content": "# Izveštaj"}}], q1)
q3 = t5.user([{"type": "tool_result", "tool_use_id": "toolu_c1", "content": "File created"}], q2)
t5.asst([{"type": "text", "text": "Izveštaj je spreman."}], q3)
tl(CW / "local_cw1/.claude/projects/-sessions-vesela-ptica" / f"{t5.sid}.jsonl", t5.rows)
w(CW / "local_cw1.json", json.dumps({"sessionId": "local_cw1", "cliSessionId": t5.sid, "title": "Izveštaj prodaje (Cowork)"}))
w(CW / "local_cw1/outputs/izvestaj.md", "# Izveštaj\n")
with open(CW / "local_cw1/vm-disk.img", "wb") as f:
    f.truncate(60_000_000)  # retka (sparse) slika diska, 60 MB

# ---------------- ~/Claude (izlazni fajlovi)
w(H / "Claude/artifact.html", "<h1>artefakt</h1>")
print("fixture OK:", H)
