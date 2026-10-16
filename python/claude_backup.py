#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Claude Backup — bezbedna kopija Claude Code razgovora, konteksta i podešavanja (macOS)

Pokretanje:
    python3 claude_backup.py           meni (strelice + Enter, razmak, ili klik mišem)
    python3 claude_backup.py --help    opcije za komandnu liniju

Garancije:
  * Program samo ČITA Claude fajlove. Ništa ne briše i ne menja u Claude-u.
  * Fajlovi se kopiraju bajt po bajt (originalni Claude format), a svaki se
    proverava SHA-256 kontrolnim zbirom — i pri kopiranju i posle, iz gotovog backupa.
  * Jedina izmena van backupa je „čuvanje istorije” (cleanupPeriodDays) — samo kad
    je ti izabereš, uz rezervnu kopiju settings.json.
  * Vraćanje iz backupa samo DODAJE fajlove koji fale. Postojeće ne prepisuje.

Radi sa python3 3.9+ koji dolazi uz macOS, bez dodatnih paketa.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import html
import io
import json
import os
import plistlib
import re
import select
import shlex
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import textwrap
import time
import unicodedata
import zipfile
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

try:
    import termios
except ImportError:  # Windows
    termios = None  # type: ignore

TOOL_VERSION = "2.0"
IS_MAC = sys.platform == "darwin"
UUID_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
VER_RE = re.compile(r'"version"\s*:\s*"(\d+\.\d+\.\d+)"')
DESKTOP_KEEP_VERSION = (2, 1, 248)   # od ove verzije Code tab/Cowork razgovori se ne brišu
RECOMMENDED_DAYS = 3650
DEFAULT_CLEANUP = 30
USER_FILES_AUTO_LIMIT = 500 * 1024 * 1024
PROJECT_BACKUP_DIR = "claude-istorija"      # folder u projektu za „istoriju ovog projekta”
PER_SESSION_DIRS = ("file-history", "tasks", "todos", "session-env")   # ~/.claude/<dir>/<session-id>…

# ~/.claude stvari koje namerno NE čuvamo (ime -> razlog)
SKIP_CLAUDE_TOP = {
    "debug": "debug logovi",
    "statsig": "analitika programa",
    "telemetry": "analitika programa",
    "shell-snapshots": "privremeni snimci terminala",
    "local": "instalacija samog Claude Code programa",
    "ide": "privremene veze sa editorom",
    "downloads": "preuzimanja ažuriranja programa",
    "sessions": "oznake trenutno upaljenih sesija",
    "cache": "keš",
    ".credentials.json": "login token — iz bezbednosnih razloga; posle vraćanja se samo ponovo uloguješ",
}
EXT_DIRS = ["skills", "agents", "commands", "plugins", "output-styles", "hooks", "rules", "workflows"]
DESKTOP_CONFIG_ITEMS = ["claude_desktop_config.json", "config.json", "Claude Extensions",
                        "Claude Extensions Settings", "extensions-installations.json"]
DESKTOP_KNOWN = set(DESKTOP_CONFIG_ITEMS) | {"claude-code-sessions", "local-agent-mode-sessions"}
PROJECT_ITEMS = ["CLAUDE.md", "CLAUDE.local.md", ".mcp.json", ".claude"]
PROJECT_SCOPED = {"code_sessions", "tmp_sessions", "project_files"}

CAT_DEFS = [
    ("code_sessions", "Claude Code razgovori", True,
     "Svi razgovori iz terminala i Code taba, po projektima — sa pod-agentima, rezultatima alata i "
     "Claude-ovom memorijom projekta. Originalni .jsonl format koji Claude čita (~/.claude/projects)."),
    ("tmp_sessions", "Slike i scratchpad iz razgovora", True,
     "Slike koje si nalepio u razgovore i Claude-ovi radni fajlovi (scratchpad). Stoje u /private/tmp "
     "i macOS ih briše pri restartu — sačuvaj ih dok postoje."),
    ("global", "Podešavanja, kontekst i istorija", True,
     "Globalni CLAUDE.md, settings.json, ~/.claude.json (projekti, MCP serveri), istorija kucanih "
     "poruka (history.jsonl), planovi, zadaci i checkpoint snimci za rewind."),
    ("extensions", "Skillovi, agenti, komande, pluginovi", True,
     "~/.claude/skills, agents, commands, plugins, output-styles, hooks i rules."),
    ("project_files", "Claude fajlovi u projektima", True,
     "Iz foldera svakog projekta samo CLAUDE.md, CLAUDE.local.md, .mcp.json i .claude/ — bez koda."),
    ("desktop_code", "Code tab — lista razgovora", True,
     "Kartice koje Claude aplikacija prikazuje u Code tabu (naslovi, arhivirano). "
     "Sami razgovori su u stavci „Claude Code razgovori”."),
    ("cowork", "Cowork sesije", True,
     "Cowork razgovori i fajlovi koje je Claude napravio ili koje si ubacio u Cowork."),
    ("desktop_config", "Podešavanja Claude aplikacije", True,
     "claude_desktop_config.json (MCP serveri), config.json i instalirane ekstenzije."),
    ("user_files", "Folderi ~/Claude i ~/Documents/Claude", False,
     "Fajlovi koje Claude aplikacija čuva za tebe (artefakti, rezultati zakazanih zadataka). "
     "Uključeno je ako je manje od 500 MB; ako je veće, proveri da tu nemaš nešto svoje (npr. kod)."),
]
CAT_TITLES = {c[0]: c[1] for c in CAT_DEFS}


# ════════════════════════════════════════════════════════════ pomoćne funkcije

def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def num(n: int) -> str:
    return f"{n:,}".replace(",", ".")


def human(n: float) -> str:
    n = float(n)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if n < 1024 or unit == "TB":
            return f"{int(n)} B" if unit == "B" else f"{n:.1f} {unit}".replace(".", ",")
        n /= 1024
    return ""


def plural(n: int, one: str, few: str, many: str) -> str:
    a, b = n % 10, n % 100
    w = one if (a == 1 and b != 11) else few if (2 <= a <= 4 and not 12 <= b <= 14) else many
    return f"{num(n)} {w}"


def n_files(n): return plural(n, "fajl", "fajla", "fajlova")
def n_convs(n): return plural(n, "razgovor", "razgovora", "razgovora")
def n_projs(n): return plural(n, "projekat", "projekta", "projekata")


def vtuple(v: str) -> tuple:
    try:
        return tuple(int(x) for x in v.split(".")[:3])
    except (ValueError, AttributeError):
        return ()


def parse_ts(ts: str) -> Optional[datetime]:
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone()
    except ValueError:
        return None


def fmt_ts(ts: str, with_time: bool = True) -> str:
    d = parse_ts(ts)
    if not d:
        return ts[:16].replace("T", " ")
    return d.strftime("%d.%m.%Y. %H:%M" if with_time else "%d.%m.%Y.")


def fmt_epoch(t: float, with_time: bool = False) -> str:
    if not t:
        return ""
    return datetime.fromtimestamp(t).strftime("%d.%m.%Y. %H:%M" if with_time else "%d.%m.%Y.")


def slug(s: str, maxlen: int = 60) -> str:
    s = nfc(s).replace("đ", "dj").replace("Đ", "Dj")
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    s = re.sub(r"[^A-Za-z0-9]+", "-", s).strip("-").lower()
    return s[:maxlen].strip("-") or "bez-naslova"


def proj_key(cwd: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "-", cwd)


def sha_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_json(p: Path) -> Any:
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def os_err(e: BaseException) -> str:
    """Razumljiva poruka za grešku pri čitanju; za macOS zabranu pristupa kaže i kako da se reši."""
    msg = getattr(e, "strerror", None) or str(e) or e.__class__.__name__
    if IS_MAC and isinstance(e, PermissionError):
        msg += (" — macOS ne dozvoljava pristup; dozvoli u System Settings → Privacy & Security → "
                "Full Disk Access (ili Files and Folders) za Terminal/python3")
    return msg


def find_tmp_root() -> Path:
    """Gde Claude Code drži slike i scratchpad: /private/tmp/claude-<uid> na Macu (ili CLAUDE_CODE_TMPDIR)."""
    uid = os.getuid() if hasattr(os, "getuid") else 0
    env = os.environ.get("CLAUDE_CODE_TMPDIR")
    if env:
        for c in (Path(env) / f"claude-{uid}", Path(env) / "claude"):
            if c.is_dir():
                return c
        return Path(env) / f"claude-{uid}"
    if IS_MAC:
        return Path("/private/tmp") / f"claude-{uid}"
    for base in (os.environ.get("TMPDIR"), "/tmp"):
        if base and (Path(base) / f"claude-{uid}").is_dir():
            return Path(base) / f"claude-{uid}"
    return Path("/tmp") / f"claude-{uid}"


class Env:
    """Sve putanje koje program koristi (HOME se poštuje, pa može i da se testira)."""

    def __init__(self):
        self.home = Path(os.path.expanduser("~"))
        cfg = os.environ.get("CLAUDE_CONFIG_DIR")
        self.claude = Path(cfg).expanduser() if cfg else self.home / ".claude"
        if cfg and (self.claude / ".claude.json").exists():
            self.claude_json = self.claude / ".claude.json"
        else:
            self.claude_json = self.home / ".claude.json"
        self.app = self.home / "Library" / "Application Support" / "Claude"
        self.tmp = find_tmp_root()
        self.managed = (Path("/Library/Application Support/ClaudeCode/managed-settings.json") if IS_MAC
                        else Path("/etc/claude-code/managed-settings.json"))
        self.script = Path(os.path.abspath(__file__))

    def token(self, p: Path) -> str:
        """Putanja zapisana tako da radi i na drugom Macu (~ i $TMP umesto korisničkog imena)."""
        s = str(p)
        for base, tok in ((self.tmp, "$TMP"), (self.home, "~")):
            b = str(base)
            if s == b:
                return tok
            if s.startswith(b + "/"):
                return tok + s[len(b):]
        return s

    def from_token(self, tok: str) -> Path:
        if tok == "$TMP" or tok.startswith("$TMP/"):
            return Path(str(self.tmp) + tok[4:])
        if tok == "~" or tok.startswith("~/"):
            return Path(str(self.home) + tok[1:])
        return Path(tok)

    def tilde(self, p) -> str:
        s, h = str(p), str(self.home)
        return nfc("~" + s[len(h):] if s == h or s.startswith(h + "/") else s)


# ════════════════════════════════════════════════════════════ skeniranje

@dataclass
class FEntry:
    src: Path
    arc: str            # putanja u backupu, npr. original/dot-claude/projects/x/y.jsonl
    cat: str
    size: int
    mtime_ns: int
    mode: int
    proj: str = ""
    link: str = ""


@dataclass
class Cat:
    id: str
    title: str
    default: bool
    desc: str
    files: List[FEntry] = field(default_factory=list)

    @property
    def size(self) -> int:
        return sum(f.size for f in self.files)

    @property
    def count(self) -> int:
        return len(self.files)


@dataclass
class Proj:
    key: str
    cwd: str = ""
    sessions: int = 0
    size: int = 0
    last: float = 0.0
    slug: str = ""


@dataclass
class Inventory:
    env: Env
    cats: Dict[str, Cat]
    projects: Dict[str, Proj]
    excluded: List[Tuple[str, str]]
    problems: List[Tuple[str, str]]
    cc_version: str
    running: List[str]


def walk_tree(root: Path, arc_root: str, cat: str, env: Env, problems: list, excluded: list,
              skip_top: Optional[Dict[str, str]] = None, ignore_top: Optional[set] = None,
              skip_rel: Optional[Callable[[str], Optional[str]]] = None,
              proj_from_top: bool = False, proj: str = "") -> List[FEntry]:
    """Prolazi kroz folder i samo ČITA metapodatke. Prati simboličke linkove (bez petlji),
    preskače specijalne fajlove (socket/pipe) i beleži sve što nije moglo da se pročita."""
    out: List[FEntry] = []
    seen = set()
    skip_top = skip_top or {}
    ignore_top = ignore_top or set()

    def add_file(p: Path, rel: str, st, link: str):
        pj = proj or (rel.split("/", 1)[0] if proj_from_top and "/" in rel else proj)
        arc = f"{arc_root}/{rel}" if rel else arc_root
        out.append(FEntry(p, arc, cat, st.st_size, st.st_mtime_ns, st.st_mode, pj, link))

    def rec(d: Path, rel: str):
        real = os.path.realpath(d)
        if real in seen:
            return
        seen.add(real)
        try:
            names = sorted(os.listdir(d))
        except OSError as e:
            problems.append((env.token(d), f"folder ne može da se otvori ({os_err(e)})"))
            return
        for n in names:
            p = d / n
            r = f"{rel}/{n}" if rel else n
            if not rel and n in ignore_top:
                continue
            if not rel and n in skip_top:
                excluded.append((env.token(p), skip_top[n]))
                continue
            if skip_rel:
                why = skip_rel(r)
                if why:
                    excluded.append((env.token(p), why))
                    continue
            is_link = os.path.islink(p)
            try:
                st = os.stat(p)
            except OSError as e:
                problems.append((env.token(p), "pokvarena prečica (link vodi na nešto što više ne postoji)"
                                 if is_link and not isinstance(e, PermissionError)
                                 else f"ne može da se pročita ({os_err(e)})"))
                continue
            link = os.readlink(p) if is_link else ""
            if stat.S_ISDIR(st.st_mode):
                rec(p, r)
            elif stat.S_ISREG(st.st_mode):
                add_file(p, r, st, link)
            else:
                excluded.append((env.token(p), "specijalan sistemski fajl (socket/pipe) — nije podatak"))

    try:
        if root.is_dir():
            rec(root, "")
        elif root.is_file():
            add_file(root, "", os.stat(root), os.readlink(root) if os.path.islink(root) else "")
    except OSError as e:
        problems.append((env.token(root), os_err(e)))
    return out


def first_cwd(p: Path) -> str:
    try:
        with open(p, encoding="utf-8", errors="replace") as fh:
            for i, line in enumerate(fh):
                if i > 80:
                    break
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                if isinstance(d, dict) and isinstance(d.get("cwd"), str) and d["cwd"]:
                    return d["cwd"]
    except OSError:
        pass
    return ""


def newest_version(files: List[FEntry]) -> str:
    best = ""
    jsonl = sorted((f for f in files if f.src.name.endswith(".jsonl")), key=lambda f: -f.mtime_ns)[:6]
    for f in jsonl:
        try:
            with open(f.src, "rb") as fh:
                fh.seek(max(0, f.size - 300_000))
                tail = fh.read().decode("utf-8", "replace")
        except OSError:
            continue
        for v in VER_RE.findall(tail):
            if vtuple(v) > vtuple(best):
                best = v
    return best


def detect_running() -> List[str]:
    found = []
    for name, label in (("Claude", "Claude aplikacija"), ("claude", "Claude Code u terminalu")):
        try:
            r = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True, timeout=5)
            pids = [x for x in r.stdout.split() if x.isdigit() and int(x) != os.getpid()]
            if pids:
                found.append(label)
        except (OSError, subprocess.SubprocessError):
            pass
    return found


def scan(env: Env) -> Inventory:
    problems: list = []
    excluded: list = []
    cats = {cid: Cat(cid, title, default, desc) for cid, title, default, desc in CAT_DEFS}
    C = env.claude

    cats["code_sessions"].files = walk_tree(C / "projects", "original/dot-claude/projects", "code_sessions",
                                            env, problems, excluded, proj_from_top=True)
    tmp = walk_tree(env.tmp, "original/claude-tmp", "tmp_sessions", env, problems, excluded, proj_from_top=True)
    cats["tmp_sessions"].files = [f for f in tmp if f.arc.count("/") >= 3]   # samo u folderima projekata

    g = walk_tree(C, "original/dot-claude", "global", env, problems, excluded,
                  skip_top=SKIP_CLAUDE_TOP, ignore_top=set(["projects"] + EXT_DIRS))
    if env.claude_json.is_file():
        g += walk_tree(env.claude_json, "original/claude.json", "global", env, problems, excluded)
    cats["global"].files = g

    ext = []
    for n in EXT_DIRS:
        ext += walk_tree(C / n, f"original/dot-claude/{n}", "extensions", env, problems, excluded)
    cats["extensions"].files = ext

    cats["desktop_code"].files = walk_tree(env.app / "claude-code-sessions",
                                           "original/claude-app/claude-code-sessions", "desktop_code",
                                           env, problems, excluded)
    cats["cowork"].files = walk_tree(env.app / "local-agent-mode-sessions",
                                     "original/claude-app/local-agent-mode-sessions", "cowork",
                                     env, problems, excluded)
    dc = []
    for n in DESKTOP_CONFIG_ITEMS:
        dc += walk_tree(env.app / n, f"original/claude-app/{n}", "desktop_config", env, problems, excluded)
    cats["desktop_config"].files = dc
    if env.app.is_dir():
        try:
            others = [n for n in sorted(os.listdir(env.app)) if n not in DESKTOP_KNOWN]
        except OSError:
            others = []
        if others:
            excluded.append((env.token(env.app) + "/(ostalo)",
                             "program, keš, logovi i virtuelna mašina aplikacije — nisu tvoji podaci"))

    uf = []
    for p in (env.home / "Claude", env.home / "Documents" / "Claude"):
        try:
            if p.is_dir():
                uf += walk_tree(p, "original/home/" + slug(env.tilde(p), 80), "user_files", env, problems, excluded)
        except OSError:
            pass
    cats["user_files"].files = uf
    # pun backup: i ovo uključi po difoltu, osim ako je ogromno (tada verovatno nije samo Claude-ovo)
    cats["user_files"].default = 0 < cats["user_files"].size <= USER_FILES_AUTO_LIMIT

    # projekti
    projects: Dict[str, Proj] = {}
    for f in cats["code_sessions"].files:
        if not f.proj:
            continue
        pr = projects.setdefault(f.proj, Proj(f.proj))
        pr.size += f.size
        pr.last = max(pr.last, f.mtime_ns / 1e9)
        name = f.src.name
        if f.arc.count("/") == 4 and name.endswith(".jsonl") and UUID_RE.match(name[:-6]):
            pr.sessions += 1
            if not pr.cwd:
                pr.cwd = first_cwd(f.src)
    cj = load_json(env.claude_json)
    if isinstance(cj, dict) and isinstance(cj.get("projects"), dict):
        for cwd in cj["projects"]:
            if isinstance(cwd, str) and cwd.startswith("/"):
                pr = projects.setdefault(proj_key(cwd), Proj(proj_key(cwd)))
                pr.cwd = pr.cwd or cwd

    pf = []
    used_slugs = set()
    for pr in sorted(projects.values(), key=lambda p: p.key):
        if not pr.cwd:
            continue
        root = Path(pr.cwd)
        try:
            if not root.is_dir():
                continue
        except OSError as e:
            problems.append((env.token(root), f"folder projekta ne može da se otvori ({os_err(e)})"))
            continue
        base = slug(root.name or "root", 40)
        s = f"{base}-{hashlib.sha1(pr.cwd.encode()).hexdigest()[:6]}"
        while s in used_slugs:
            s += "x"
        used_slugs.add(s)
        pr.slug = s
        arc_base = f"original/projekti/{s}"
        is_home = os.path.realpath(root) == os.path.realpath(env.home)
        for n in PROJECT_ITEMS:
            p = root / n
            if n == ".claude":
                if is_home or os.path.realpath(p) == os.path.realpath(C):
                    continue   # to je globalni ~/.claude, već je u drugim stavkama
                pf += walk_tree(p, f"{arc_base}/.claude", "project_files", env, problems, excluded, proj=pr.key,
                                skip_rel=lambda r: ("kopije koda (git worktrees)"
                                                    if r == "worktrees" or r.startswith("worktrees/") else None))
            else:
                try:
                    exists = os.path.isfile(p) or os.path.islink(p)
                    os.stat(p) if exists else None
                except OSError as e:
                    problems.append((env.token(p), f"ne može da se pročita ({os_err(e)})"))
                    continue
                if exists:
                    pf += walk_tree(p, f"{arc_base}/{n}", "project_files", env, problems, excluded, proj=pr.key)
    cats["project_files"].files = pf

    return Inventory(env, cats, projects, excluded, problems,
                     newest_version(cats["code_sessions"].files), detect_running())


def select_entries(inv: Inventory, cats: List[str], projects: Optional[set]) -> List[FEntry]:
    out, seen = [], set()
    for cid in cats:
        for f in inv.cats[cid].files:
            if projects is not None and cid in PROJECT_SCOPED and f.proj not in projects:
                continue
            if f.arc in seen:
                continue
            seen.add(f.arc)
            out.append(f)
    return out


def cat_note(inv: Inventory, cid: str, projects: Optional[set] = None) -> str:
    c = inv.cats[cid]
    files = c.files if projects is None or cid not in PROJECT_SCOPED else \
        [f for f in c.files if f.proj in projects]
    if not files:
        return "nije pronađeno"
    size = human(sum(f.size for f in files))
    if cid == "code_sessions":
        n = sum(1 for f in files if f.arc.count("/") == 4 and f.src.name.endswith(".jsonl")
                and UUID_RE.match(f.src.name[:-6]))
        return f"{n_convs(n)} · {size}"
    if cid == "extensions":
        parts = []
        for d, one, few, many in (("skills", "skill", "skilla", "skillova"), ("agents", "agent", "agenta", "agenata"),
                                  ("commands", "komanda", "komande", "komandi")):
            p = inv.env.claude / d
            try:
                k = len([x for x in os.listdir(p) if not x.startswith(".")]) if p.is_dir() else 0
            except OSError:
                k = 0
            if k:
                parts.append(plural(k, one, few, many))
        return " · ".join(parts + [size]) if parts else f"{n_files(len(files))} · {size}"
    return f"{n_files(len(files))} · {size}"


# ---------- jedan projekat

def is_session_file(f: FEntry) -> bool:
    n = f.src.name
    return f.cat == "code_sessions" and f.arc.count("/") == 4 and n.endswith(".jsonl") and bool(UUID_RE.match(n[:-6]))


def project_for_path(inv: Inventory, path) -> Optional[Proj]:
    """Projekat kome pripada folder (ili njegov podfolder). Sam home folder se ne računa kao projekat."""
    try:
        rp = os.path.realpath(str(path))
        home = os.path.realpath(inv.env.home)
    except OSError:
        return None
    if rp == home:
        return None
    best, best_len = None, -1
    for p in inv.projects.values():
        if not p.cwd or not p.sessions:
            continue
        c = os.path.realpath(p.cwd)
        if c == home or c == "/":
            continue
        if (rp == c or rp.startswith(c + "/")) and len(c) > best_len:
            best, best_len = p, len(c)
    return best


def project_entries(inv: Inventory, key: str) -> List[FEntry]:
    """Sve što pripada jednom projektu: razgovori (sa pod-agentima, rezultatima alata i memorijom projekta),
    slike i scratchpad, Claude fajlovi iz foldera projekta, checkpoint snimci i zadaci tih razgovora,
    i kartice iz Code taba. Sve ostaje u originalnom formatu, pa može i da se vrati."""
    pr = inv.projects.get(key)
    out = [f for f in inv.cats["code_sessions"].files if f.proj == key]
    sids = {f.src.name[:-6] for f in out if is_session_file(f)}
    out += [f for f in inv.cats["tmp_sessions"].files if f.proj == key]
    out += [f for f in inv.cats["project_files"].files if f.proj == key]
    if sids:
        for f in inv.cats["global"].files:
            parts = f.arc.split("/")
            if len(parts) >= 4 and parts[1] == "dot-claude" and parts[2] in PER_SESSION_DIRS:
                nm = parts[3]
                if nm in sids or any(nm.startswith(s) for s in sids):
                    out.append(f)
    cwd = pr.cwd if pr else ""
    for f in inv.cats["desktop_code"].files:
        if f.src.suffix == ".json" and f.size < 2_000_000:
            d = load_json(f.src)
            if isinstance(d, dict) and (d.get("cliSessionId") in sids or (cwd and d.get("cwd") == cwd)):
                out.append(f)
    seen, uniq = set(), []
    for f in out:
        if f.arc not in seen:
            seen.add(f.arc)
            uniq.append(f)
    return uniq


def project_prompt_history(env: Env, cwd: str) -> bytes:
    """Redovi iz ~/.claude/history.jsonl (poruke koje si kucao) koji pripadaju ovom projektu."""
    out = []
    try:
        with open(env.claude / "history.jsonl", encoding="utf-8", errors="replace") as fh:
            for line in fh:
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                if isinstance(d, dict) and d.get("project") == cwd:
                    out.append(line if line.endswith("\n") else line + "\n")
    except OSError:
        return b""
    return "".join(out).encode("utf-8")


def prepare_project_dest(dest: Path) -> Optional[str]:
    """Pravi <projekat>/claude-istorija/ sa .gitignore, da istorija (može da sadrži ključeve) ne ode u git."""
    try:
        dest.mkdir(exist_ok=True)
        gi = dest / ".gitignore"
        if not os.path.lexists(gi):
            with open(gi, "x", encoding="utf-8") as f:
                f.write("# Claude istorija — namerno van git-a (može da sadrži lozinke i API ključeve)\n*\n")
    except OSError as e:
        return os_err(e)
    return None


# ════════════════════════════════════════════════════════════ čuvanje istorije

def settings_path(env: Env) -> Path:
    return env.claude / "settings.json"


def read_settings(env: Env) -> Tuple[Optional[dict], Optional[str], str]:
    p = settings_path(env)
    if not p.exists():
        return {}, None, ""
    try:
        raw = p.read_text(encoding="utf-8")
    except OSError as e:
        return None, f"ne mogu da pročitam {env.tilde(p)} ({e.strerror or e})", ""
    if not raw.strip():
        return {}, None, raw
    try:
        d = json.loads(raw)
    except ValueError as e:
        return None, f"{env.tilde(p)} nije ispravan JSON ({e}) — ne diram ga", raw
    if not isinstance(d, dict):
        return None, f"{env.tilde(p)} nema očekivani oblik — ne diram ga", raw
    return d, None, raw


def retention_info(env: Env, version: str) -> dict:
    d, err, _ = read_settings(env)
    cleanup = d.get("cleanupPeriodDays") if d else None
    desktop = d.get("desktopSessionCleanupPeriodDays") if d else None
    m = load_json(env.managed)
    managed = m.get("cleanupPeriodDays") if isinstance(m, dict) else None
    if isinstance(managed, int):
        effective = managed
    elif isinstance(cleanup, int) and not isinstance(cleanup, bool):
        effective = cleanup
    else:
        effective = DEFAULT_CLEANUP
    vt = vtuple(version)
    desktop_kept = bool(vt) and vt >= DESKTOP_KEEP_VERSION and desktop is None and managed is None
    return dict(cleanup=cleanup, desktop=desktop, managed=managed, effective=effective, error=err,
                version=version, desktop_kept=desktop_kept, version_known=bool(vt))


def _patch_json_text(raw: str, key: str, value: int, before: dict) -> str:
    """Menja samo jedan ključ, a ostatak teksta ostavlja kakav jeste. Ako ne može sigurno — piše ceo JSON."""
    expected = dict(before)
    expected[key] = value
    if raw.strip():
        if key in before:
            pat = re.compile(r'("%s"\s*:\s*)(-?\d+(?:\.\d+)?|null|true|false|"[^"]*")' % re.escape(key))
            new, n = pat.subn(lambda m: m.group(1) + str(value), raw, count=1)
        else:
            i = raw.find("{")
            j = raw.find("}", i)
            if i >= 0 and raw[i + 1:].strip().startswith("}"):
                new = raw[:i] + '{\n  "%s": %d\n}' % (key, value) + raw[j + 1:]
            elif i >= 0:
                m = re.search(r'\n([ \t]+)"', raw[i:])
                new = raw[:i + 1] + '\n%s"%s": %d,' % (m.group(1) if m else "  ", key, value) + raw[i + 1:]
            else:
                new = ""
        try:
            if new and json.loads(new) == expected:
                return new if new.endswith("\n") else new + "\n"
        except ValueError:
            pass
    return json.dumps(expected, indent=2, ensure_ascii=False) + "\n"


def set_retention(env: Env, days: int) -> Tuple[bool, str, Optional[Path], List[str]]:
    """Podešava cleanupPeriodDays. Pre izmene pravi kopiju settings.json, piše atomski i proverava rezultat."""
    if not isinstance(days, int) or days < 1:
        return False, "broj dana mora biti 1 ili više", None, []
    d, err, raw = read_settings(env)
    if d is None:
        return False, err or "settings.json ne može da se pročita", None, []
    p = settings_path(env)
    changes = []
    new_raw, data = raw, dict(d)
    old = d.get("cleanupPeriodDays")
    new_raw = _patch_json_text(new_raw, "cleanupPeriodDays", days, data)
    data["cleanupPeriodDays"] = days
    changes.append(f'"cleanupPeriodDays": {old if old is not None else "(nije podešeno = 30)"} → {days}')
    desk = d.get("desktopSessionCleanupPeriodDays")
    if isinstance(desk, int) and desk < days:
        new_raw = _patch_json_text(new_raw, "desktopSessionCleanupPeriodDays", days, data)
        data["desktopSessionCleanupPeriodDays"] = days
        changes.append(f'"desktopSessionCleanupPeriodDays": {desk} → {days}')
    if old == days and len(changes) == 1:
        return True, "već je tako podešeno — ništa nije menjano", None, []
    backup = None
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        if p.exists():
            backup = p.with_name(f"settings.json.pre-claude-backup-{datetime.now():%Y%m%d-%H%M%S}")
            k = 2
            while backup.exists():
                backup = p.with_name(f"settings.json.pre-claude-backup-{datetime.now():%Y%m%d-%H%M%S}-{k}")
                k += 1
            shutil.copy2(p, backup)
        tmp = p.with_name(f".settings.json.claude-backup-{os.getpid()}.tmp")
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(new_raw)
            f.flush()
            os.fsync(f.fileno())
        if p.exists():
            os.chmod(tmp, stat.S_IMODE(os.stat(p).st_mode))
        os.replace(tmp, p)
        check = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        return False, f"izmena nije uspela ({e}); tvoj fajl nije diran", backup, []
    if check != data:
        if backup:
            shutil.copy2(backup, p)
        return False, "provera posle upisa nije prošla — vraćen je stari fajl", backup, []
    return True, "sačuvano i provereno", backup, changes


# ════════════════════════════════════════════════════════════ čitljiva verzija razgovora

REMINDER_RE = re.compile(r"<system-reminder>.*?</system-reminder>", re.S)
ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")


@dataclass
class Item:
    kind: str              # text, thinking, tool, image, command, output, notice, summary, raw
    text: str = ""
    name: str = ""
    data: Any = None
    result: Optional[list] = None
    is_error: bool = False


@dataclass
class Turn:
    role: str              # user, assistant, divider
    ts: str
    items: List[Item] = field(default_factory=list)


@dataclass
class Conv:
    sid: str
    src: Path
    kind: str
    proj: str
    arc: str = ""
    title: str = ""
    first: str = ""
    cwd: str = ""
    start: str = ""
    end: str = ""
    models: List[str] = field(default_factory=list)
    entry: str = ""
    version: str = ""
    turns: List[Turn] = field(default_factory=list)
    bad: int = 0


def image_of(b: dict):
    s = b.get("source") if isinstance(b, dict) else None
    if isinstance(s, dict) and s.get("type") == "base64" and s.get("data"):
        return (s.get("media_type") or "image/png", s["data"])
    return None


def result_parts(content) -> list:
    if content is None:
        return []
    if isinstance(content, str):
        return [("text", content)]
    out = []
    if isinstance(content, list):
        for b in content:
            if not isinstance(b, dict):
                out.append(("text", str(b)))
            elif b.get("type") == "text":
                out.append(("text", b.get("text", "")))
            elif b.get("type") == "image":
                img = image_of(b)
                out.append(("image", img) if img else ("text", "[slika]"))
            else:
                out.append(("text", json.dumps(b, ensure_ascii=False, indent=2)))
    else:
        out.append(("text", json.dumps(content, ensure_ascii=False, indent=2)))
    return out


def parse_user_text(text: str) -> List[Item]:
    text = REMINDER_RE.sub("", text)
    text = re.sub(r"<local-command-caveat>.*?</local-command-caveat>", "", text, flags=re.S)
    items: List[Item] = []
    m = re.search(r"<command-name>(.*?)</command-name>", text, re.S)
    if m:
        a = re.search(r"<command-args>(.*?)</command-args>", text, re.S)
        items.append(Item("command", (m.group(1).strip() + " " + (a.group(1).strip() if a else "")).strip()))
        text = re.sub(r"<command-(name|message|args)>.*?</command-\1>", "", text, flags=re.S)
    for tag in ("bash-input", "local-command-stdout", "local-command-stderr", "bash-stdout", "bash-stderr"):
        for mm in re.finditer(rf"<{tag}>(.*?)</{tag}>", text, re.S):
            val = ANSI_RE.sub("", mm.group(1)).strip()
            if val:
                items.append(Item("command", "! " + val) if tag == "bash-input" else Item("output", val))
        text = re.sub(rf"<{tag}>.*?</{tag}>", "", text, flags=re.S)
    t = text.strip()
    if t:
        items.append(Item("text", t))
    return items


def parse_conv(path: Path, kind: str, proj: str, titles: Dict[str, str]) -> Optional[Conv]:
    c = Conv(sid=path.stem, src=path, kind=kind, proj=proj)
    custom = ai = summary = None
    first_user = ""
    tools: Dict[str, Item] = {}

    def cur(role: str, ts: str) -> Turn:
        if c.turns and c.turns[-1].role == role:
            return c.turns[-1]
        t = Turn(role, ts)
        c.turns.append(t)
        return t

    try:
        fh = open(path, encoding="utf-8", errors="replace")
    except OSError:
        return None
    with fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                d = json.loads(line)
            except ValueError:
                c.bad += 1
                continue
            if not isinstance(d, dict):
                c.bad += 1
                continue
            t = d.get("type")
            if t == "custom-title":
                custom = d.get("customTitle") or d.get("title") or custom
                continue
            if isinstance(d.get("aiTitle"), str) and d["aiTitle"].strip():
                ai = d["aiTitle"]
            if t == "summary":
                summary = summary or d.get("summary")
                continue
            if not c.cwd and isinstance(d.get("cwd"), str):
                c.cwd = d["cwd"]
            if isinstance(d.get("version"), str):
                c.version = d["version"]
            if not c.entry and isinstance(d.get("entrypoint"), str):
                c.entry = d["entrypoint"]
            if t not in ("user", "assistant", "system"):
                continue
            ts = d.get("timestamp") or ""
            if ts:
                c.start = c.start or ts
                c.end = ts
            if d.get("isSidechain"):
                continue
            if t == "system":
                st = d.get("subtype") or ""
                if st == "compact_boundary":
                    c.turns.append(Turn("divider", ts, [Item("notice", "Razgovor je sažet (compact) — Claude nastavlja na osnovu sažetka")]))
                elif d.get("level") in ("error", "warning") or st in ("api_error", "local_command"):
                    txt = d.get("content")
                    if isinstance(txt, str) and REMINDER_RE.sub("", txt).strip():
                        cur("assistant", ts).items.append(Item("notice", ANSI_RE.sub("", REMINDER_RE.sub("", txt)).strip(),
                                                               is_error=d.get("level") == "error"))
                continue
            if d.get("isMeta"):
                continue
            msg = d.get("message")
            if not isinstance(msg, dict):
                continue
            content = msg.get("content")
            if isinstance(content, str):
                content = [{"type": "text", "text": content}]
            if not isinstance(content, list):
                continue
            if t == "assistant":
                mdl = msg.get("model")
                if isinstance(mdl, str) and mdl and mdl != "<synthetic>" and mdl not in c.models:
                    c.models.append(mdl)
                turn = cur("assistant", ts)
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    bt = b.get("type") or ""
                    if bt == "text":
                        if (b.get("text") or "").strip():
                            turn.items.append(Item("text", b["text"], is_error=bool(d.get("isApiErrorMessage"))))
                    elif bt == "thinking":
                        if (b.get("thinking") or "").strip():
                            turn.items.append(Item("thinking", b["thinking"]))
                    elif bt == "redacted_thinking":
                        continue
                    elif bt in ("tool_use", "server_tool_use", "mcp_tool_use"):
                        it = Item("tool", name=str(b.get("name") or "alat"), data=b.get("input"))
                        turn.items.append(it)
                        if b.get("id"):
                            tools[b["id"]] = it
                    elif bt.endswith("_tool_result"):
                        target = tools.get(b.get("tool_use_id"))
                        parts = result_parts(b.get("content"))
                        if target is not None:
                            target.result = (target.result or []) + parts
                        else:
                            turn.items.append(Item("tool", name=bt, result=parts))
                    else:
                        turn.items.append(Item("raw", json.dumps(b, ensure_ascii=False, indent=2), name=bt or "podaci"))
            else:
                if d.get("isCompactSummary"):
                    txt = "\n".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
                    c.turns.append(Turn("divider", ts, [Item("summary", REMINDER_RE.sub("", txt).strip())]))
                    continue
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    bt = b.get("type")
                    if bt == "tool_result":
                        target = tools.get(b.get("tool_use_id"))
                        parts = result_parts(b.get("content"))
                        if target is not None:
                            target.result = (target.result or []) + parts
                            target.is_error = target.is_error or bool(b.get("is_error"))
                        else:
                            cur("assistant", ts).items.append(Item("tool", name="rezultat alata", result=parts,
                                                                   is_error=bool(b.get("is_error"))))
                    elif bt == "text":
                        its = parse_user_text(b.get("text", ""))
                        if its:
                            cur("user", ts).items.extend(its)
                            if not first_user:
                                first_user = next((i.text for i in its if i.kind in ("text", "command")), "")
                    elif bt == "image":
                        img = image_of(b)
                        cur("user", ts).items.append(Item("image", data=img) if img else Item("notice", "[slika]"))
                    elif bt == "document":
                        cur("user", ts).items.append(Item("notice", "[priložen dokument" +
                                                          (f": {b.get('title')}" if b.get("title") else "") + "]"))
                    else:
                        cur("user", ts).items.append(Item("raw", json.dumps(b, ensure_ascii=False, indent=2), name=str(bt)))
    c.turns = [t for t in c.turns if t.items]
    if not any(t.role in ("user", "assistant") for t in c.turns):
        return None
    c.first = first_user.strip()
    first_line = c.first.splitlines()[0][:90] if c.first else ""
    c.title = nfc(str(custom or titles.get(c.sid) or ai or summary or first_line or "(bez naslova)").strip())
    return c


def load_card_titles(dirs: List[Path]) -> Dict[str, str]:
    """Naslovi razgovora onako kako ih prikazuje Claude aplikacija (Code tab / Cowork kartice)."""
    titles: Dict[str, str] = {}
    for root in dirs:
        if not root.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            if len(Path(dirpath).relative_to(root).parts) >= 4:
                dirnames[:] = []
            for fn in filenames:
                if not fn.endswith(".json"):
                    continue
                p = Path(dirpath) / fn
                try:
                    if p.stat().st_size > 2_000_000:
                        continue
                except OSError:
                    continue
                d = load_json(p)
                if isinstance(d, dict) and isinstance(d.get("title"), str) and d["title"].strip():
                    for k in ("cliSessionId", "sessionId"):
                        if isinstance(d.get(k), str):
                            titles.setdefault(d[k], d["title"].strip())
    return titles


def source_label(c: Conv) -> str:
    if c.kind == "cowork":
        return "Cowork"
    e = (c.entry or "").lower()
    if "desktop" in e:
        return "Code tab"
    if e == "cli":
        return "Terminal"
    if any(x in e for x in ("vscode", "jetbrains", "ide")):
        return "Editor"
    if "cowork" in e:
        return "Cowork"
    if "remote" in e or "web" in e:
        return "Cloud"
    if "sdk" in e:
        return "SDK"
    return e or "—"


def tool_hint(name: str, inp) -> str:
    if not isinstance(inp, dict):
        return ""
    for k in ("command", "file_path", "path", "pattern", "url", "query", "description", "prompt", "skill", "notebook_path"):
        v = inp.get(k)
        if isinstance(v, str) and v.strip():
            v = " ".join(v.split())
            return v if len(v) <= 110 else v[:110] + "…"
    return ""


def fence(text: str, lang: str = "") -> str:
    longest = max((len(m) for m in re.findall(r"`+", text)), default=0)
    f = "`" * max(3, longest + 1)
    return f"{f}{lang}\n{text}\n{f}"


# ---------- HTML

CSS = """
:root{--bg:#faf9f5;--fg:#1f1e1d;--muted:#6b6a66;--card:#fff;--line:#e5e2d9;--accent:#c4633f;--user:#f1eee5;--code:#f4f2ec;--err:#b3261e;--ok:#2f7d3a}
@media (prefers-color-scheme:dark){:root{--bg:#1c1b1a;--fg:#ecebe6;--muted:#a19f99;--card:#262523;--line:#3a3936;--accent:#e2896a;--user:#2d2b28;--code:#232220;--err:#f28b82;--ok:#81c995}}
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}
.wrap{max-width:940px;margin:0 auto;padding:24px 16px 80px}
a{color:var(--accent)}
.back{display:inline-block;margin-bottom:16px;text-decoration:none;font-size:14px}
h1{font-size:24px;line-height:1.25;margin:0 0 14px;overflow-wrap:anywhere}
.head{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:18px 20px;margin-bottom:28px}
.meta{display:grid;grid-template-columns:max-content 1fr;gap:3px 14px;font-size:13px;color:var(--muted);margin:0}
.meta dt{white-space:nowrap}.meta dd{margin:0;color:var(--fg);overflow-wrap:anywhere}
.resume{margin-top:12px;font-size:13px;color:var(--muted)}
.resume code{user-select:all;-webkit-user-select:all;display:block;margin-top:4px;padding:8px 10px;background:var(--code);border-radius:6px;overflow-x:auto;white-space:pre;color:var(--fg)}
.turn{margin:22px 0}
.who{font-size:12px;font-weight:600;color:var(--muted);letter-spacing:.03em;margin-bottom:6px;display:flex;gap:10px;align-items:baseline}
.who time{font-weight:400}
.user .bubble{background:var(--user);border-radius:12px;padding:12px 16px;border-left:3px solid var(--accent)}
.md{white-space:pre-wrap;overflow-wrap:anywhere}
.md .h{font-weight:700}.md .h1{font-size:1.3em}.md .h2{font-size:1.15em}
pre{margin:8px 0;padding:10px 12px;background:var(--code);border-radius:8px;overflow-x:auto;font:12.5px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;white-space:pre}
.md pre{white-space:pre}
code{font:.9em ui-monospace,SFMono-Regular,Menlo,monospace;background:var(--code);padding:1px 4px;border-radius:4px}
pre code{background:none;padding:0;font-size:inherit}
details{margin:6px 0;border:1px solid var(--line);border-radius:8px;background:var(--card)}
details>summary{cursor:pointer;padding:6px 10px;font-size:13px;color:var(--muted);list-style:none;overflow-wrap:anywhere}
details>summary::-webkit-details-marker{display:none}
details>summary::before{content:"▸ "}details[open]>summary::before{content:"▾ "}
details .body{padding:2px 12px 10px}
.tool summary b{color:var(--fg);font-weight:600}
.tool.err{border-color:var(--err)}.tool.err summary{color:var(--err)}
.label{font-size:11px;font-weight:700;color:var(--muted);text-transform:uppercase;letter-spacing:.05em;margin-top:8px}
.kv{font-size:13px;margin:2px 0}.kv span{color:var(--muted)}
.divider{text-align:center;color:var(--muted);font-size:13px;margin:30px 0;display:flex;align-items:center;gap:12px}
.divider::before,.divider::after{content:"";flex:1;border-top:1px dashed var(--line)}
.notice{color:var(--muted);font-size:13px;font-style:italic;white-space:pre-wrap}
.err-text,.notice.err{color:var(--err)}
.cmd{font:13px ui-monospace,Menlo,monospace;color:var(--accent);white-space:pre-wrap}
img.shot{max-width:100%;border-radius:8px;border:1px solid var(--line);margin:6px 0;display:block}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px;margin:18px 0 22px}
.tile{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 14px}
.tile b{display:block;font-size:22px;line-height:1.2}.tile span{font-size:12px;color:var(--muted)}
input#q{width:100%;padding:10px 12px;font-size:15px;border:1px solid var(--line);border-radius:8px;background:var(--card);color:var(--fg);margin-bottom:16px}
.proj{margin:10px 0}.proj>summary{font-size:14px;color:var(--fg);padding:10px 12px}.proj>summary span{color:var(--muted);font-weight:400}
table{width:100%;border-collapse:collapse;font-size:14px}
td{padding:8px 10px;border-top:1px solid var(--line);vertical-align:top}
td.d{white-space:nowrap;color:var(--muted);font-size:13px;width:1%}td.n{white-space:nowrap;color:var(--muted);font-size:12px;text-align:right;width:1%}
td a{text-decoration:none;font-weight:600;overflow-wrap:anywhere}.sub{color:var(--muted);font-size:12.5px;margin-top:2px;overflow-wrap:anywhere}
.badge{display:inline-block;font-size:11px;padding:1px 6px;border-radius:99px;border:1px solid var(--line);color:var(--muted);margin-left:6px;font-weight:400}
.note{font-size:13px;color:var(--muted)}
@media (max-width:600px){td.n{display:none}.meta{grid-template-columns:1fr}.meta dt{margin-top:6px}}
"""


def _inline(s: str) -> str:
    s = re.sub(r"`([^`\n]+)`", r"<code>\1</code>", s)
    s = re.sub(r"\*\*([^*\n]+)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"\[([^\]\n]+)\]\((https?://[^)\s]+)\)", r'<a href="\2">\1</a>', s)
    return s


def md_to_html(text: str) -> str:
    """Lagan prikaz markdowna: kod blokovi, `kod`, **bold**, naslovi. Tekst ostaje doslovno isti."""
    out, buf, code = [], [], None
    fence_mark, lang = "", ""

    def flush():
        if not buf:
            return
        lines = []
        for ln in html.escape("\n".join(buf)).split("\n"):
            m = re.match(r"(#{1,6}) (.*)", ln)
            if m:
                lines.append(f'<span class="h h{len(m.group(1))}">{_inline(m.group(2))}</span>')
            else:
                lines.append(_inline(ln))
        out.append('<div class="md">' + "\n".join(lines) + "</div>")
        buf.clear()

    for ln in text.split("\n"):
        s = ln.lstrip()
        if code is None and s.startswith("```"):
            flush()
            fence_mark = re.match(r"`{3,}", s).group(0)
            lang = s[len(fence_mark):].strip()
            code = []
        elif code is not None and s.startswith(fence_mark) and not s.strip("`").strip():
            out.append(f'<pre><code>{html.escape(chr(10).join(code))}</code></pre>')
            code = None
        elif code is not None:
            code.append(ln)
        else:
            buf.append(ln)
    if code is not None:
        out.append(f'<pre><code>{html.escape(chr(10).join(code))}</code></pre>')
    flush()
    return "".join(out)


def html_input(inp) -> str:
    E = html.escape
    if isinstance(inp, dict):
        parts = []
        for k, v in inp.items():
            if isinstance(v, str) and ("\n" in v or len(v) > 90):
                parts.append(f'<div class="kv"><span>{E(str(k))}:</span></div><pre>{E(v)}</pre>')
            elif isinstance(v, str):
                parts.append(f'<div class="kv"><span>{E(str(k))}:</span> <code>{E(v)}</code></div>')
            else:
                parts.append(f'<div class="kv"><span>{E(str(k))}:</span></div><pre>{E(json.dumps(v, ensure_ascii=False, indent=2))}</pre>')
        return "".join(parts)
    return f"<pre>{E(json.dumps(inp, ensure_ascii=False, indent=2))}</pre>"


def html_item(it: Item, role: str) -> str:
    E = html.escape
    if it.kind == "text":
        cls = " err-text" if it.is_error else ""
        return f'<div class="{cls.strip()}">{md_to_html(it.text)}</div>' if cls else md_to_html(it.text)
    if it.kind == "command":
        return f'<div class="cmd">{E(it.text)}</div>'
    if it.kind == "output":
        return f"<pre>{E(it.text)}</pre>"
    if it.kind == "thinking":
        return f'<details class="think"><summary>Razmišljanje</summary><div class="body"><div class="md">{E(it.text)}</div></div></details>'
    if it.kind == "image":
        mt, data = it.data
        return f'<img class="shot" alt="slika" src="data:{E(mt)};base64,{data}">'
    if it.kind == "notice":
        return f'<div class="notice{" err" if it.is_error else ""}">{E(it.text)}</div>'
    if it.kind == "summary":
        return f'<details><summary>Sažetak ranijeg dela razgovora</summary><div class="body">{md_to_html(it.text)}</div></details>'
    if it.kind == "tool":
        hint = tool_hint(it.name, it.data)
        body = ""
        if it.data is not None:
            body += '<div class="label">Ulaz</div>' + html_input(it.data)
        if it.result:
            body += '<div class="label">Rezultat</div>'
            for k, v in it.result:
                if k == "image":
                    body += f'<img class="shot" alt="slika" src="data:{E(v[0])};base64,{v[1]}">'
                else:
                    body += f"<pre>{E(v)}</pre>"
        err = " err" if it.is_error else ""
        h = f" <code>{E(hint)}</code>" if hint else ""
        return (f'<details class="tool{err}"><summary><b>{E(it.name)}</b>{h}{" — greška" if it.is_error else ""}'
                f'</summary><div class="body">{body}</div></details>')
    return f'<details><summary>{E(it.name or "podaci")}</summary><div class="body"><pre>{E(it.text)}</pre></div></details>'


def html_page(title: str, body: str) -> bytes:
    return ("<!doctype html><html lang=\"sr\"><head><meta charset=\"utf-8\">"
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
            f"<title>{html.escape(title)}</title><style>{CSS}</style></head><body><div class=\"wrap\">"
            f"{body}</div></body></html>").encode("utf-8")


def resume_cmd(c: Conv) -> str:
    return f"cd {shlex.quote(c.cwd)} && claude --resume {c.sid}" if c.cwd else f"claude --resume {c.sid}"


def html_conv(c: Conv, back: str, env: Env) -> bytes:
    E = html.escape
    meta = [("Projekat", env.tilde(c.cwd) if c.cwd else "—"), ("Početak", fmt_ts(c.start)),
            ("Poslednja poruka", fmt_ts(c.end)),
            ("Poruke", f"ti: {sum(1 for t in c.turns if t.role == 'user')} · "
                       f"Claude: {sum(1 for t in c.turns if t.role == 'assistant')}"),
            ("Izvor", source_label(c)), ("Model", ", ".join(c.models) or "—"),
            ("Session ID", c.sid), ("Original", c.arc)]
    if c.bad:
        verb = "nije mogao" if c.bad % 10 == 1 and c.bad % 100 != 11 else "nisu mogla" if 2 <= c.bad % 10 <= 4 and not 12 <= c.bad % 100 <= 14 else "nije moglo"
        meta.append(("Napomena", f"{plural(c.bad, 'oštećen red', 'oštećena reda', 'oštećenih redova')} u originalu "
                                 f"{verb} da se prikaže (original je sačuvan netaknut)"))
    dl = "".join(f"<dt>{E(k)}</dt><dd>{E(v)}</dd>" for k, v in meta)
    parts = [f'<a class="back" href="{back}">← Svi razgovori</a>',
             f'<header class="head"><h1>{E(c.title)}</h1><dl class="meta">{dl}</dl>'
             f'<div class="resume">Nastavi ovaj razgovor u terminalu:<code>{E(resume_cmd(c))}</code></div></header>']
    for t in c.turns:
        if t.role == "divider":
            for it in t.items:
                parts.append(html_item(it, "divider") if it.kind == "summary"
                             else f'<div class="divider">{E(it.text)}</div>')
            continue
        who = "Ti" if t.role == "user" else "Claude"
        inner = "".join(html_item(i, t.role) for i in t.items)
        parts.append(f'<section class="turn {t.role}"><div class="who"><span>{who}</span>'
                     f'<time>{E(fmt_ts(t.ts))}</time></div><div class="bubble">{inner}</div></section>')
    return html_page(c.title, "".join(parts))


def html_index(groups: List[Tuple[str, List[Tuple[Conv, str]]]], env: Env, created: str, has_md: bool) -> bytes:
    E = html.escape
    convs = [c for _, lst in groups for c, _ in lst]
    starts = sorted(x for x in (parse_ts(c.start) for c in convs) if x)
    period = f"{starts[0]:%d.%m.%Y.} – {starts[-1]:%d.%m.%Y.}" if starts else "—"
    msgs = sum(sum(1 for t in c.turns if t.role == "user") for c in convs)
    tiles = (f'<div class="tiles"><div class="tile"><b>{num(len(convs))}</b><span>razgovora</span></div>'
             f'<div class="tile"><b>{num(len(groups))}</b><span>projekata</span></div>'
             f'<div class="tile"><b>{num(msgs)}</b><span>tvojih poruka</span></div>'
             f'<div class="tile"><b style="font-size:15px">{E(period)}</b><span>period</span></div></div>')
    rows = []
    for label, lst in groups:
        trs = []
        for c, href in lst:
            snippet = " ".join(c.first.split())[:180]
            s = E(f"{c.title} {snippet} {label}".lower(), quote=True)
            n = sum(1 for t in c.turns if t.role == "user")
            trs.append(f'<tr data-s="{s}"><td class="d">{E(fmt_ts(c.start, False))}</td>'
                       f'<td><a href="{E(href, quote=True)}">{E(c.title)}</a>'
                       f'{"" if source_label(c) == "—" else f"<span class=badge>{E(source_label(c))}</span>"}'
                       f'{f"<div class=sub>{E(snippet)}</div>" if snippet and snippet != c.title else ""}</td>'
                       f'<td class="n">{n} poruka</td></tr>')
        rows.append(f'<details class="proj" open><summary><b>{E(label)}</b> <span>· {n_convs(len(lst))}</span></summary>'
                    f'<table>{"".join(trs)}</table></details>')
    js = ("<script>const q=document.getElementById('q');q.addEventListener('input',()=>{const v=q.value.trim().toLowerCase();"
          "document.querySelectorAll('.proj').forEach(g=>{let any=false;g.querySelectorAll('tr[data-s]').forEach(r=>{"
          "const m=!v||r.dataset.s.includes(v);r.hidden=!m;if(m)any=true;});g.hidden=!any;if(v&&any)g.open=true;});});</script>")
    body = (f"<h1>Claude razgovori</h1><div class=\"note\">Backup napravljen {E(created)}. "
            f"Originalni Claude fajlovi su u folderu <code>original/</code> — vidi PROCITAJ_ME.txt."
            f"{' Markdown verzija je u folderu <code>citljivo-md/</code>.' if has_md else ''}</div>"
            f"{tiles}<input id=\"q\" type=\"search\" placeholder=\"Pretraži naslove, poruke i projekte…\" autocomplete=\"off\">"
            f"{''.join(rows) or '<p class=note>Nema razgovora u ovom backupu.</p>'}{js}")
    return html_page("Claude razgovori", body)


# ---------- Markdown

def md_input(inp) -> str:
    if isinstance(inp, dict):
        out = []
        for k, v in inp.items():
            if isinstance(v, str) and ("\n" in v or len(v) > 90):
                out.append(f"*{k}:*\n\n{fence(v)}")
            elif isinstance(v, str):
                out.append(f"*{k}:* `{v}`" if "`" not in v else f"*{k}:* {v}")
            else:
                out.append(f"*{k}:*\n\n{fence(json.dumps(v, ensure_ascii=False, indent=2), 'json')}")
        return "\n\n".join(out)
    return fence(json.dumps(inp, ensure_ascii=False, indent=2), "json")


def md_conv(c: Conv, env: Env, put_image: Callable[[str, str], str]) -> bytes:
    L = [f"# {c.title}", "",
         "| | |", "|---|---|",
         f"| Projekat | `{env.tilde(c.cwd) if c.cwd else '—'}` |",
         f"| Početak | {fmt_ts(c.start)} |", f"| Poslednja poruka | {fmt_ts(c.end)} |",
         f"| Izvor | {source_label(c)} |", f"| Model | {', '.join(c.models) or '—'} |",
         f"| Session ID | `{c.sid}` |", f"| Original | `{c.arc}` |", "",
         "Nastavi u terminalu:", "", fence(resume_cmd(c), "bash"), "", "---", ""]
    for t in c.turns:
        if t.role == "divider":
            for it in t.items:
                if it.kind == "summary":
                    L += ["<details><summary>Sažetak ranijeg dela razgovora</summary>", "", it.text, "", "</details>", ""]
                else:
                    L += [f"--- *{it.text}* ---", ""]
            continue
        L += [f"## {'🧑 Ti' if t.role == 'user' else '🤖 Claude'} · {fmt_ts(t.ts)}", ""]
        for it in t.items:
            if it.kind == "text":
                L += [it.text, ""]
            elif it.kind == "command":
                L += [f"`{it.text}`" if "`" not in it.text and "\n" not in it.text else fence(it.text), ""]
            elif it.kind == "output":
                L += [fence(it.text), ""]
            elif it.kind == "thinking":
                L += ["<details><summary>Razmišljanje</summary>", "", it.text, "", "</details>", ""]
            elif it.kind == "image":
                L += [f"![slika]({put_image(*it.data)})", ""]
            elif it.kind == "notice":
                L += [f"*{it.text}*", ""]
            elif it.kind == "tool":
                hint = tool_hint(it.name, it.data)
                summ = f"🔧 {it.name}" + (f" — {html.escape(hint)}" if hint else "") + (" — greška" if it.is_error else "")
                L += [f"<details><summary>{summ}</summary>", ""]
                if it.data is not None:
                    L += ["**Ulaz**", "", md_input(it.data), ""]
                if it.result:
                    L += ["**Rezultat**", ""]
                    for k, v in it.result:
                        L += [f"![slika]({put_image(*v)})" if k == "image" else fence(v), ""]
                L += ["</details>", ""]
            else:
                L += [f"<details><summary>{it.name or 'podaci'}</summary>", "", fence(it.text, "json"), "", "</details>", ""]
    return "\n".join(L).encode("utf-8")


# ════════════════════════════════════════════════════════════ backup motor

ZIP64_LIMIT = (1 << 31) - 1


def zip_dt(mtime_ns: int) -> tuple:
    t = time.localtime(max(mtime_ns / 1e9, 315_620_000))   # ZIP ne zna datume pre 1980.
    return t[:6]


class ZipSink:
    def __init__(self, final: Path, root: str):
        self.final, self.root = final, root
        self.part = final.with_name(final.name + ".part")
        self.zf = zipfile.ZipFile(self.part, "x", zipfile.ZIP_DEFLATED, allowZip64=True)

    def put(self, arc: str, fobj, size: int, mtime_ns: int, mode: int):
        zi = zipfile.ZipInfo(f"{self.root}/{arc}", date_time=zip_dt(mtime_ns))
        zi.compress_type = zipfile.ZIP_DEFLATED
        zi.external_attr = ((stat.S_IFREG | (mode & 0o777)) & 0xFFFF) << 16
        with self.zf.open(zi, "w", force_zip64=size > ZIP64_LIMIT) as w:
            shutil.copyfileobj(fobj, w, 1 << 20)

    def put_bytes(self, arc: str, data: bytes):
        self.put(arc, io.BytesIO(data), len(data), time.time_ns(), 0o600)

    def close(self) -> Path:
        self.zf.close()
        os.chmod(self.part, 0o600)
        if self.final.exists():
            raise OSError(f"{self.final} već postoji")
        os.rename(self.part, self.final)
        return self.final

    def abort(self):
        try:
            self.zf.close()
        except Exception:
            pass
        if self.part.name.startswith("ClaudeBackup_") and self.part.name.endswith(".part"):
            try:
                os.remove(self.part)   # samo naš nedovršeni fajl
            except OSError:
                pass


class DirSink:
    def __init__(self, final: Path, root: str):
        self.final, self.root = final, root
        self.part = final.with_name(final.name + ".part")
        os.makedirs(self.part, mode=0o700)

    def put(self, arc: str, fobj, size: int, mtime_ns: int, mode: int):
        t = self.part / arc
        t.parent.mkdir(parents=True, exist_ok=True)
        with open(t, "xb") as w:
            shutil.copyfileobj(fobj, w, 1 << 20)
        try:
            os.chmod(t, mode & 0o777)
            os.utime(t, ns=(mtime_ns, mtime_ns))
        except OSError:
            pass

    def put_bytes(self, arc: str, data: bytes):
        self.put(arc, io.BytesIO(data), len(data), time.time_ns(), 0o600)

    def close(self) -> Path:
        if self.final.exists():
            raise OSError(f"{self.final} već postoji")
        os.rename(self.part, self.final)
        return self.final

    def abort(self):
        if self.part.name.startswith("ClaudeBackup_") and self.part.name.endswith(".part"):
            shutil.rmtree(self.part, ignore_errors=True)   # samo naš nedovršeni folder


def capture(src: Path):
    """Pročita fajl u privremenu memoriju i izračuna SHA-256. Ako se fajl menjao dok je čitan
    (npr. Claude upravo piše), pokuša ponovo. Vraća (spool, sha, veličina, stabilan, mtime_ns, mode)."""
    for attempt in range(3):
        st1 = os.stat(src)
        if not stat.S_ISREG(st1.st_mode):
            raise OSError("nije običan fajl")
        spool = tempfile.SpooledTemporaryFile(max_size=64 << 20)
        h = hashlib.sha256()
        n = 0
        try:
            with open(src, "rb") as f:
                while True:
                    b = f.read(1 << 20)
                    if not b:
                        break
                    h.update(b)
                    spool.write(b)
                    n += len(b)
            st2 = os.stat(src)
        except BaseException:
            spool.close()
            raise
        stable = st1.st_mtime_ns == st2.st_mtime_ns and st1.st_size == st2.st_size == n
        if stable or attempt == 2:
            return spool, h.hexdigest(), n, stable, st1.st_mtime_ns if stable else st2.st_mtime_ns, st2.st_mode
        spool.close()
        time.sleep(0.3)
    raise OSError("neočekivano")


def unique_name(dest: Path, as_zip: bool, prefix: str = "ClaudeBackup") -> str:
    stamp = datetime.now().strftime("%Y-%m-%d_%H%M%S")
    base, n = f"{prefix}_{stamp}", 1
    while any(os.path.lexists(dest / (base + ext)) for ext in (".zip", ".zip.part", "", ".part")):
        n += 1
        base = f"{prefix}_{stamp}_{n}"
    return base


@dataclass
class VerifyResult:
    ok: int = 0
    total: int = 0
    bytes: int = 0
    bad: List[str] = field(default_factory=list)
    missing: List[str] = field(default_factory=list)
    error: str = ""
    manifest: Optional[dict] = None

    @property
    def passed(self) -> bool:
        return not self.error and not self.bad and not self.missing and self.ok == self.total and self.total > 0


@dataclass
class BackupResult:
    path: Path
    size: int
    entries: List[dict]
    generated: List[dict]
    failed: List[Tuple[str, str]]
    unstable: List[str]
    convs: int
    verify: VerifyResult
    new_after: List[str]
    changed_after: List[str]
    per_cat: Dict[str, Tuple[int, int]]
    excluded: List[Tuple[str, str]]
    problems: List[Tuple[str, str]]
    formats: set
    seconds: float


def build_readable(inv: Inventory, entries: List[FEntry], formats: set, gen: Callable[[str, bytes], None],
                   cb: Optional[Callable]) -> int:
    env = inv.env
    srcs = []
    for e in entries:
        nm = e.src.name
        if not nm.endswith(".jsonl") or not UUID_RE.match(nm[:-6]):
            continue
        if e.cat == "code_sessions" and e.arc.count("/") == 4:
            srcs.append((e, "code"))
        elif e.cat == "cowork":
            srcs.append((e, "cowork"))
    if not srcs:
        return 0
    titles = load_card_titles([env.app / "claude-code-sessions", env.app / "local-agent-mode-sessions"])
    convs: List[Conv] = []
    for i, (e, kind) in enumerate(srcs):
        if cb:
            cb("read", i / len(srcs), f"{i + 1}/{len(srcs)}", e.src.name)
        c = parse_conv(e.src, kind, e.proj if kind == "code" else "cowork", titles)
        if c:
            c.arc = e.arc
            convs.append(c)
    # grupe po projektu
    groups: Dict[str, List[Conv]] = {}
    for c in convs:
        groups.setdefault(c.proj, []).append(c)
    labels, dirs, used = {}, {}, set()
    for key, lst in groups.items():
        if key == "cowork":
            label = "Cowork"
        else:
            pr = inv.projects.get(key)
            cwd = (pr.cwd if pr and pr.cwd else "") or next((c.cwd for c in lst if c.cwd), "")
            label = env.tilde(cwd) if cwd else key
        labels[key] = label
        d = slug(Path(label).name if label not in ("Cowork",) else "cowork", 40) or "projekat"
        base, k = d, 2
        while d in used:
            d, k = f"{base}-{k}", k + 1
        used.add(d)
        dirs[key] = d
    order = sorted(groups, key=lambda k: max((c.end or c.start) for c in groups[k]), reverse=True)
    html_groups, md_rows = [], []
    for gi, key in enumerate(order):
        lst = sorted(groups[key], key=lambda c: c.start, reverse=True)
        hg = []
        for c in lst:
            d = parse_ts(c.start)
            fname = f"{d:%Y-%m-%d}_" if d else ""
            fname += f"{slug(c.title, 50)}_{c.sid[:8]}"
            if cb:
                cb("render", gi / max(1, len(order)), labels[key], c.title)
            if "html" in formats:
                gen(f"citljivo-html/{dirs[key]}/{fname}.html", html_conv(c, "../../Pregled.html", env))
                hg.append((c, f"citljivo-html/{dirs[key]}/{fname}.html"))
            if "md" in formats:
                counter = [0]

                def put_image(media: str, data: str, _d=dirs[key], _sid=c.sid[:8], _c=counter) -> str:
                    _c[0] += 1
                    ext = {"image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp"}.get(media, "png")
                    rel = f"slike/{_sid}-{_c[0]}.{ext}"
                    try:
                        gen(f"citljivo-md/{_d}/{rel}", base64.b64decode(data))
                    except (ValueError, TypeError):
                        return "(slika nije mogla da se dekodira)"
                    return rel
                gen(f"citljivo-md/{dirs[key]}/{fname}.md", md_conv(c, env, put_image))
                md_rows.append((labels[key], c, f"{dirs[key]}/{fname}.md"))
        if hg:
            html_groups.append((labels[key], hg))
    created = datetime.now().strftime("%d.%m.%Y. u %H:%M")
    if "html" in formats:
        gen("Pregled.html", html_index(html_groups, env, created, "md" in formats))
    if "md" in formats:
        L = ["# Claude razgovori", "", f"{n_convs(len(convs))} · backup napravljen {created}", ""]
        last = None
        for label, c, rel in md_rows:
            if label != last:
                L += ["", f"## {label}", "", "| Datum | Razgovor | Izvor | Poruka |", "|---|---|---|---|"]
                last = label
            t = c.title.replace("|", "/")
            L.append(f"| {fmt_ts(c.start, False)} | [{t}]({rel.replace(' ', '%20')}) | {source_label(c)} | "
                     f"{sum(1 for x in c.turns if x.role == 'user')} |")
        gen("citljivo-md/INDEX.md", ("\n".join(L) + "\n").encode("utf-8"))
    return len(convs)


def readme_text(env: Env, formats: set, created: str, scope: Optional[dict] = None) -> str:
    lines = [f"CLAUDE BACKUP — {created}", "=" * 40, ""]
    if scope and scope.get("type") == "project":
        lines += [f"Ovo je istorija JEDNOG projekta: {env.tilde(scope.get('cwd', ''))}",
                  "(razgovori, memorija projekta, slike, checkpoint snimci i Code tab kartice tog projekta)", ""]
    lines += ["Šta je ovde", "-----------"]
    if scope and scope.get("type") == "project":
        lines.append("kucane-poruke.jsonl  Poruke koje si kucao u ovom projektu (iz ~/.claude/history.jsonl).")
    if "html" in formats:
        lines.append("Pregled.html      Svi razgovori za čitanje u browseru (dupli klik).")
    if "md" in formats:
        lines.append("citljivo-md/      Isti razgovori kao Markdown (.md) fajlovi; spisak je u INDEX.md.")
    lines += [
        "original/         TAČNE kopije Claude fajlova, bajt po bajt. Iz ovoga se vraća:",
        "  dot-claude/        = ~/.claude (razgovori, memorija, podešavanja, skillovi…)",
        "  claude.json        = ~/.claude.json",
        "  claude-app/        = ~/Library/Application Support/Claude (Code tab lista, Cowork…)",
        "  claude-tmp/        = /private/tmp/claude-<broj> (slike i scratchpad iz razgovora)",
        "  projekti/          = CLAUDE.md, .claude/ i .mcp.json iz foldera projekata",
        "                       (tačna putanja svakog projekta je u izvestaj.txt)",
        "MANIFEST.json     Spisak svih fajlova: odakle su, veličina, vreme izmene, SHA-256.",
        "SHA256SUMS.txt    Kontrolni zbirovi svih fajlova.",
        "izvestaj.txt      Šta je sačuvano, šta nije i zašto.",
        "",
        "Folderi čije ime počinje tačkom (.claude) su u Finderu sakriveni — Cmd+Shift+. ih prikazuje.",
        "",
        "Kako da proverim da je backup ispravan",
        "--------------------------------------",
        "python3 claude_backup.py   →  „Proveri backup”",
        "ili u Terminalu, u ovom folderu:   shasum -a 256 -c SHA256SUMS.txt",
        "",
        "Kako da vratim",
        "--------------",
        "1. Zatvori Claude (Cmd+Q).",
        "2. python3 claude_backup.py  →  „Vrati iz backupa”  →  izaberi ovaj backup.",
        "   Program samo DODAJE fajlove koji fale. Tvoje postojeće fajlove ne dira.",
        "3. Na novom Macu stavi projekte na iste putanje kao ranije, da bi",
        "   „claude --resume” u tom folderu pronašao stare razgovore.",
        "",
        "Privatnost",
        "----------",
        "Razgovori mogu da sadrže lozinke i API ključeve koje si nalepio ili koje je Claude",
        "pročitao iz .env fajlova. Čuvaj ovaj backup kao privatan.",
        "Login token (~/.claude/.credentials.json) namerno nije sačuvan — posle vraćanja se",
        "samo ponovo uloguješ.",
        "",
        "Chat tab razgovori nisu deo ovog backupa: oni su na Anthropic serveru.",
        "Izvoz: claude.ai → Settings → Privacy → Export data.",
        "",
        f"Napravio claude_backup.py v{TOOL_VERSION}",
    ]
    return "\n".join(lines) + "\n"


def report_text(inv: Inventory, files: List[dict], failed, unstable, per_cat, projects: Optional[set],
                created: str, formats: set, convs: int) -> str:
    env = inv.env
    L = [f"IZVEŠTAJ — Claude Backup {created}", "=" * 40, "",
         f"Računar: {socket.gethostname()}   Korisnik: {env.home.name}   Claude Code: {inv.cc_version or 'nepoznato'}", "",
         "Sačuvano", "--------"]
    for cid, (n, b) in per_cat.items():
        L.append(f"  ✔ {CAT_TITLES[cid]:<40} {n_files(n):>14}   {human(b):>10}")
    L += ["", f"Ukupno: {n_files(len(files))}, {human(sum(f['size'] for f in files))}. "
              "Svaki fajl je kopiran bajt po bajt i proveren SHA-256 zbirom.", ""]
    if convs:
        fm = " + ".join(x for x, k in (("HTML (Pregled.html)", "html"), ("Markdown (citljivo-md/)", "md")) if k in formats)
        L += [f"Čitljiva verzija: {n_convs(convs)} — {fm}", ""]
    L += ["Projekti", "--------"]
    for pr in sorted(inv.projects.values(), key=lambda p: -p.last):
        mark = "✔" if projects is None or pr.key in projects else "–"
        L.append(f"  {mark} {env.tilde(pr.cwd) if pr.cwd else pr.key}")
        L.append(f"      razgovori: ~/.claude/projects/{pr.key}   ({n_convs(pr.sessions)})")
        if pr.slug:
            L.append(f"      Claude fajlovi projekta u backupu: original/projekti/{pr.slug}/")
    if failed:
        L += ["", "NIJE MOGLO DA SE PROČITA (nije u backupu)", "-----------------------------------------"]
        L += [f"  ✖ {p} — {why}" for p, why in failed]
    if unstable:
        L += ["", "Menjali su se dok je backup radio (Claude je bio upaljen) — sačuvana je poslednja verzija:"]
        L += [f"  ⚠ {p}" for p in unstable]
    if inv.problems:
        L += ["", "Upozorenja pri skeniranju", "-------------------------"]
        L += [f"  ⚠ {p} — {why}" for p, why in inv.problems]
    L += ["", "Namerno nije sačuvano", "---------------------"]
    seen = set()
    for p, why in inv.excluded:
        if (p, why) not in seen:
            seen.add((p, why))
            L.append(f"  – {p} — {why}")
    L += ["  – kod projekata — po dogovoru; čuvaju se samo Claude fajlovi iz projekata",
          "  – Chat tab — nalazi se na Anthropic serveru (Settings → Privacy → Export data)"]
    return "\n".join(L) + "\n"


def run_backup(inv: Inventory, cats: List[str], projects: Optional[set], formats: set, dest: Path,
               as_zip: bool, cb: Optional[Callable] = None, *,
               selector: Optional[Callable[[Inventory], List[FEntry]]] = None, prefix: str = "ClaudeBackup",
               scope: Optional[dict] = None, extra: Optional[Callable[[Callable[[str, bytes], None]], None]] = None
               ) -> BackupResult:
    """Pravi backup. Izvorne fajlove samo čita; piše isključivo u novi ClaudeBackup_… fajl/folder u `dest`."""
    env = inv.env
    t0 = time.time()
    pick = selector or (lambda i: select_entries(i, cats, projects))
    entries = pick(inv)
    if selector:
        cats = list(dict.fromkeys(e.cat for e in entries))
    name = unique_name(dest, as_zip, prefix)
    final = dest / (name + (".zip" if as_zip else ""))
    sink = ZipSink(final, name) if as_zip else DirSink(final, name)
    files_meta: List[dict] = []
    generated: List[dict] = []
    failed: List[Tuple[str, str]] = []
    unstable: List[str] = []
    per_cat: Dict[str, Tuple[int, int]] = {}
    total = sum(e.size for e in entries) or 1
    done = 0
    created = datetime.now().strftime("%d.%m.%Y. u %H:%M")

    def gen(arc: str, data: bytes):
        sink.put_bytes(arc, data)
        generated.append({"arc": arc, "size": len(data), "sha256": sha_bytes(data)})

    try:
        for i, e in enumerate(entries):
            if cb:
                cb("copy", done / total, f"{num(i + 1)} / {num(len(entries))} fajlova · {human(done)} / {human(total)}",
                   env.tilde(e.src))
            try:
                spool, digest, size, stable, mt, mode = capture(e.src)
            except OSError as ex:
                failed.append((env.token(e.src), os_err(ex)))
                continue
            with spool:
                spool.seek(0)
                sink.put(e.arc, spool, size, mt, mode)
            rec = {"arc": e.arc, "src": env.token(e.src), "cat": e.cat, "size": size, "sha256": digest,
                   "mtime_ns": mt, "mode": stat.S_IMODE(mode)}
            if e.proj:
                rec["proj"] = e.proj
            if e.cat == "project_files" and e.proj in inv.projects:
                rec["proj_root"] = env.token(Path(inv.projects[e.proj].cwd))
            if e.link:
                rec["symlink"] = e.link
            files_meta.append(rec)
            n, b = per_cat.get(e.cat, (0, 0))
            per_cat[e.cat] = (n + 1, b + size)
            if not stable:
                unstable.append(rec["src"])
            done += e.size
        convs = 0
        if formats & {"html", "md"}:
            convs = build_readable(inv, entries, formats, gen, cb)
        if extra:
            extra(gen)
        gen("PROCITAJ_ME.txt", readme_text(env, formats, created, scope).encode("utf-8"))
        gen("izvestaj.txt", report_text(inv, files_meta, failed, unstable, per_cat, projects, created,
                                        formats, convs).encode("utf-8"))
        manifest = {
            "format": "claude-backup", "tool_version": TOOL_VERSION,
            "scope": scope or {"type": "full"},
            "created": datetime.now(timezone.utc).isoformat(), "hostname": socket.gethostname(),
            "home": str(env.home), "tmp_root": str(env.tmp), "claude_code_version": inv.cc_version,
            "categories": {cid: {"title": CAT_TITLES[cid], "files": per_cat.get(cid, (0, 0))[0],
                                 "bytes": per_cat.get(cid, (0, 0))[1]} for cid in cats},
            "projects": {k: {"cwd": p.cwd, "selected": projects is None or k in projects, "slug": p.slug}
                         for k, p in inv.projects.items()},
            "formats": sorted(formats | {"original"}),
            "files": files_meta, "generated": generated,
            "failed": [{"src": p, "error": w} for p, w in failed], "changed_during_backup": unstable,
            "excluded": [{"path": p, "reason": w} for p, w in dict.fromkeys(inv.excluded)],
        }
        sink.put_bytes("MANIFEST.json", json.dumps(manifest, ensure_ascii=False, indent=1).encode("utf-8"))
        sums = "".join(f"{f['sha256']}  {f['arc']}\n" for f in files_meta + generated)
        sink.put_bytes("SHA256SUMS.txt", sums.encode("utf-8"))
        out = sink.close()
    except BaseException:
        sink.abort()
        raise
    vr = verify_backup(out, cb)
    # da li je nešto nastalo ili se promenilo posle kopiranja?
    if cb:
        cb("rescan", 1.0, "Poredim backup sa trenutnim stanjem…", "")
    now = pick(scan(env))
    saved = {f["src"]: f for f in files_meta}
    new_after = [env.token(e.src) for e in now if env.token(e.src) not in saved]
    changed_after = [env.token(e.src) for e in now if env.token(e.src) in saved
                     and (e.size != saved[env.token(e.src)]["size"] or e.mtime_ns != saved[env.token(e.src)]["mtime_ns"])]
    size = out.stat().st_size if out.is_file() else sum(f["size"] for f in files_meta + generated)
    return BackupResult(out, size, files_meta, generated, failed, unstable, convs, vr, new_after, changed_after,
                        per_cat, list(dict.fromkeys(inv.excluded)), inv.problems, formats, time.time() - t0)


# ---------- čitanje backupa (provera i vraćanje)

def safe_arc(arc: str) -> bool:
    return bool(arc) and not arc.startswith("/") and ".." not in arc.split("/") and "\\" not in arc


class Reader:
    def __init__(self, path: Path):
        self.path = path
        self.zf = None
        if path.is_dir():
            self.root = path
            if not (path / "MANIFEST.json").is_file():
                raise ValueError("u ovom folderu nema MANIFEST.json — nije backup napravljen ovim programom")
        else:
            self.zf = zipfile.ZipFile(path)
            names = self.zf.namelist()
            mans = [n for n in names if n.endswith("/MANIFEST.json") and n.count("/") == 1]
            if not mans and "MANIFEST.json" in names:
                mans = ["MANIFEST.json"]
            if not mans:
                raise ValueError("u ZIP-u nema MANIFEST.json — nije backup napravljen ovim programom")
            self.prefix = mans[0][: -len("MANIFEST.json")]
            self.names = set(names)

    def manifest(self) -> dict:
        raw = self.zf.read(self.prefix + "MANIFEST.json") if self.zf else (self.root / "MANIFEST.json").read_bytes()
        m = json.loads(raw.decode("utf-8"))
        if not isinstance(m, dict) or m.get("format") != "claude-backup":
            raise ValueError("MANIFEST.json nije prepoznat")
        return m

    def has(self, arc: str) -> bool:
        return (self.prefix + arc) in self.names if self.zf else (self.root / arc).is_file()

    def open(self, arc: str):
        return self.zf.open(self.prefix + arc) if self.zf else open(self.root / arc, "rb")

    def close(self):
        if self.zf:
            self.zf.close()


def verify_backup(path: Path, cb: Optional[Callable] = None) -> VerifyResult:
    vr = VerifyResult()
    try:
        rd = Reader(path)
        man = rd.manifest()
    except Exception as e:   # oštećen zip, pogrešan fajl…
        vr.error = str(e) or e.__class__.__name__
        return vr
    vr.manifest = man
    entries = list(man.get("files", [])) + list(man.get("generated", []))
    vr.total = len(entries)
    total = sum(int(e.get("size", 0)) for e in entries) or 1
    done = 0
    for i, e in enumerate(entries):
        arc = str(e.get("arc", ""))
        if cb and (i % 7 == 0 or i == len(entries) - 1):
            cb("verify", done / total, f"{num(i + 1)} / {num(len(entries))} fajlova", arc)
        if not safe_arc(arc):
            vr.bad.append(arc)
            continue
        if not rd.has(arc):
            vr.missing.append(arc)
            continue
        h, n = hashlib.sha256(), 0
        try:
            with rd.open(arc) as f:
                for chunk in iter(lambda: f.read(1 << 20), b""):
                    h.update(chunk)
                    n += len(chunk)
        except Exception:
            vr.bad.append(arc)
            continue
        if h.hexdigest() == e.get("sha256") and n == e.get("size"):
            vr.ok += 1
            vr.bytes += n
        else:
            vr.bad.append(arc)
        done += int(e.get("size", 0))
    rd.close()
    return vr


@dataclass
class RItem:
    e: dict
    target: Optional[Path]
    status: str        # add / same / diff / noproj / unsafe


def restore_target(env: Env, e: dict) -> Tuple[Optional[Path], str]:
    tok = str(e.get("src", ""))
    if ".." in tok.split("/") or not tok:
        return None, "unsafe"
    t = Path(os.path.abspath(env.from_token(tok)))
    if e.get("cat") == "project_files":
        root_tok = str(e.get("proj_root", ""))
        if not root_tok:
            return None, "unsafe"
        root = Path(os.path.abspath(env.from_token(root_tok)))
        if not str(t).startswith(str(root) + "/"):
            return None, "unsafe"
        rel = str(t)[len(str(root)) + 1:]
        if not (rel in ("CLAUDE.md", "CLAUDE.local.md", ".mcp.json") or rel.startswith(".claude/")):
            return None, "unsafe"
        if not root.is_dir():
            return t, "noproj"
        return t, "ok"
    allowed = [env.home, env.tmp, env.claude]
    if not any(str(t).startswith(str(a) + "/") for a in allowed):
        return None, "unsafe"
    return t, "ok"


def plan_restore(env: Env, man: dict, cb: Optional[Callable] = None) -> List[RItem]:
    items = []
    files = man.get("files", [])
    for i, e in enumerate(files):
        if cb and i % 11 == 0:
            cb("plan", i / max(1, len(files)), f"{num(i + 1)} / {num(len(files))}", str(e.get("src", "")))
        if not safe_arc(str(e.get("arc", ""))):
            items.append(RItem(e, None, "unsafe"))
            continue
        t, st = restore_target(env, e)
        if st != "ok":
            items.append(RItem(e, t, st))
            continue
        if not os.path.lexists(t):
            items.append(RItem(e, t, "add"))
            continue
        try:
            same = t.is_file() and t.stat().st_size == e.get("size") and sha_file(t) == e.get("sha256")
        except OSError:
            same = False
        items.append(RItem(e, t, "same" if same else "diff"))
    return items


def apply_restore(rd: Reader, items: List[RItem], cb: Optional[Callable] = None) -> Tuple[int, List[Tuple[str, str]]]:
    todo = [it for it in items if it.status == "add"]
    added, failed = 0, []
    for i, it in enumerate(todo):
        e, t = it.e, it.target
        if cb:
            cb("restore", i / max(1, len(todo)), f"{num(i + 1)} / {num(len(todo))}", str(e.get("src")))
        try:
            t.parent.mkdir(parents=True, exist_ok=True)
            h = hashlib.sha256()
            with rd.open(e["arc"]) as src, open(t, "xb") as dst:     # "x" = nikad ne prepisuje postojeći fajl
                for chunk in iter(lambda: src.read(1 << 20), b""):
                    h.update(chunk)
                    dst.write(chunk)
            if h.hexdigest() != e.get("sha256"):
                os.remove(t)   # brišemo samo fajl koji smo upravo mi napravili
                failed.append((str(e.get("src")), "kontrolni zbir se ne slaže"))
                continue
            mt = int(e.get("mtime_ns") or time.time_ns())
            os.utime(t, ns=(mt, mt))
            os.chmod(t, int(e.get("mode", 0o600)) & 0o777)
            added += 1
        except FileExistsError:
            it.status = "diff"
        except Exception as ex:
            failed.append((str(e.get("src")), str(ex)))
    return added, failed


def find_backups(env: Env, extra: Optional[List[Path]] = None) -> List[Path]:
    locs = [env.home / "Desktop", env.home / "Documents", env.home / "Downloads", env.home / "ClaudeBackups"]
    try:
        for v in sorted(Path("/Volumes").iterdir()):
            if os.path.realpath(v) != "/":
                locs += [v, v / "ClaudeBackups"]
    except OSError:
        pass
    locs += extra or []
    out, seen = [], set()
    for loc in dict.fromkeys(locs):
        if not os.path.isdir(loc):
            continue
        try:
            for p in loc.iterdir():
                if not p.name.startswith("ClaudeBackup_") or p.name.endswith(".part"):
                    continue
                if (p.suffix == ".zip" and p.is_file()) or (p.is_dir() and (p / "MANIFEST.json").is_file()):
                    rp = os.path.realpath(p)
                    if rp not in seen:
                        seen.add(rp)
                        out.append(p)
        except OSError:
            continue
    def mt(p):
        try:
            return p.stat().st_mtime
        except OSError:
            return 0
    return sorted(out, key=mt, reverse=True)


def make_launcher(env: Env) -> Tuple[bool, str]:
    desk = env.home / "Desktop"
    if not desk.is_dir():
        return False, "Ne postoji folder Desktop."
    target = desk / "Claude Backup.command"
    marker = "# claude_backup.py launcher"
    if target.exists():
        try:
            content = target.read_text(encoding="utf-8", errors="replace")
        except OSError:
            content = ""
        if marker not in content:
            return False, f"Na Desktopu već postoji „{target.name}” koji nije napravio ovaj program — ne diram ga."
    py = sys.executable or "/usr/bin/python3"
    text = f"#!/bin/bash\n{marker}\ncd \"$HOME\"\nexec {shlex.quote(py)} {shlex.quote(str(env.script))}\n"
    with open(target, "w", encoding="utf-8") as f:
        f.write(text)
    os.chmod(target, 0o755)
    return True, (f"Napravljeno: ~/Desktop/{target.name}. Dupli klik otvara ovaj program u Terminalu. "
                  f"Ne pomeraj {env.script.name} iz {env.tilde(env.script.parent)} — ikonica pokazuje na njega.")


# ════════════════════════════════════════════════════════════ automatski backup (macOS launchd)

SCHED_LABEL = "com.claudebackup.auto"
BACKUP_ZIP_RE = re.compile(r"^ClaudeBackup_\d{4}-\d{2}-\d{2}_\d{6}(_\d+)?\.zip$")
FREQ_TEXT = {"daily": "svakog dana", "weekly": "svakog ponedeljka", "monthly": "1. u mesecu"}


def sched_paths(env: Env) -> Tuple[Path, Path, Path]:
    plist = env.home / "Library" / "LaunchAgents" / f"{SCHED_LABEL}.plist"
    tool = env.home / "Library" / "Application Support" / "ClaudeBackupTool" / "claude_backup.py"
    log = env.home / "Library" / "Logs" / "claude-backup.log"
    return plist, tool, log


def _launchctl(*args: str) -> Tuple[int, str]:
    try:
        r = subprocess.run(["launchctl", *args], capture_output=True, text=True, timeout=20)
        return r.returncode, (r.stderr or r.stdout).strip()
    except (OSError, subprocess.SubprocessError) as e:
        return 1, str(e)


def schedule_status(env: Env) -> Optional[dict]:
    plist, _, log = sched_paths(env)
    if not plist.is_file():
        return None
    try:
        with open(plist, "rb") as f:
            d = plistlib.load(f)
    except Exception:
        return {"broken": True}
    args = [str(a) for a in d.get("ProgramArguments", [])]
    cal = d.get("StartCalendarInterval") or {}

    def arg(name, default=""):
        return args[args.index(name) + 1] if name in args and args.index(name) + 1 < len(args) else default
    freq = "monthly" if "Day" in cal else "weekly" if "Weekday" in cal else "daily"
    loaded = _launchctl("print", f"gui/{os.getuid()}/{SCHED_LABEL}")[0] == 0 if IS_MAC else False
    return {"freq": freq, "hour": int(cal.get("Hour", 0)), "minute": int(cal.get("Minute", 0)),
            "dest": arg("--dest"), "keep": int(arg("--keep", "0") or 0), "loaded": loaded, "log": str(log)}


def describe_schedule(env: Env, st: dict) -> str:
    keep = ("čuva sve" if not st["keep"] else
            "čuva " + plural(st["keep"], "poslednji backup", "poslednja backupa", "poslednjih backupa"))
    return (f"{FREQ_TEXT[st['freq']]} u {st['hour']:02d}:{st['minute']:02d} → "
            f"{env.tilde(st['dest'])} · {keep}")


def install_schedule(env: Env, freq: str, hour: int, minute: int, dest: Path, keep: int) -> Tuple[bool, str]:
    """Pravi macOS LaunchAgent. Kopija skripte ide u ~/Library/Application Support/ClaudeBackupTool,
    jer macOS pozadinskim programima ne dozvoljava čitanje iz Downloads/Documents/Desktop."""
    if not IS_MAC:
        return False, "Automatski backup preko launchd radi samo na macOS-u."
    plist, tool, log = sched_paths(env)
    try:
        dest.mkdir(parents=True, exist_ok=True)
        tool.parent.mkdir(parents=True, exist_ok=True)
        if os.path.realpath(tool) != os.path.realpath(env.script):
            shutil.copy2(env.script, tool)
        log.parent.mkdir(parents=True, exist_ok=True)
        plist.parent.mkdir(parents=True, exist_ok=True)
    except OSError as e:
        return False, f"Ne mogu da pripremim fajlove: {e}"
    args = [sys.executable or "/usr/bin/python3", str(tool), "--backup", "--scheduled", "--if-due", "--notify",
            "--dest", str(dest), "--format", "html"]
    if keep:
        args += ["--keep", str(keep)]
    cal: Dict[str, int] = {"Hour": hour, "Minute": minute}
    if freq == "weekly":
        cal["Weekday"] = 1
    elif freq == "monthly":
        cal["Day"] = 1
    data = {"Label": SCHED_LABEL, "ProgramArguments": args, "StartCalendarInterval": cal,
            "StandardOutPath": str(log), "StandardErrorPath": str(log), "ProcessType": "Background",
            "LowPriorityIO": True, "Nice": 10,
            # RunAtLoad: pokreće se i kad se uloguješ / upališ Mac — ako je termin propušten (Mac bio ugašen),
            # --if-due napravi backup odmah; ako nije propušten, ne radi ništa.
            "RunAtLoad": True,
            "EnvironmentVariables": {"LANG": "en_US.UTF-8", "PYTHONIOENCODING": "utf-8"}}
    try:
        with open(plist, "wb") as f:
            plistlib.dump(data, f)
    except OSError as e:
        return False, f"Ne mogu da upišem {env.tilde(plist)}: {e}"
    uid = os.getuid()
    _launchctl("bootout", f"gui/{uid}/{SCHED_LABEL}")
    rc, err = _launchctl("bootstrap", f"gui/{uid}", str(plist))
    if rc != 0:
        rc, err = _launchctl("load", "-w", str(plist))
    if rc != 0:
        return False, f"macOS nije prihvatio zakazivanje: {err}"
    return True, "Uključeno."


def remove_schedule(env: Env) -> Tuple[bool, str]:
    plist, _, _ = sched_paths(env)
    if IS_MAC:
        rc, _ = _launchctl("bootout", f"gui/{os.getuid()}/{SCHED_LABEL}")
        if rc != 0:
            _launchctl("unload", "-w", str(plist))
    if plist.is_file():
        try:
            with open(plist, "rb") as f:
                if plistlib.load(f).get("Label") != SCHED_LABEL:
                    return False, "Fajl zakazivanja nije napravio ovaj program — ne diram ga."
            os.remove(plist)   # samo naš fajl zakazivanja
        except Exception as e:
            return False, f"Ne mogu da uklonim {env.tilde(plist)}: {e}"
    return True, "Automatski backup je isključen. Postojeći backupi su ostali gde jesu."


def force_flag_path(env: Env) -> Path:
    return sched_paths(env)[1].parent / "run-now.flag"


def run_schedule_now(env: Env) -> Tuple[bool, str]:
    try:   # znak da ovo pokretanje ne čeka termin
        force_flag_path(env).parent.mkdir(parents=True, exist_ok=True)
        force_flag_path(env).write_text("1", encoding="utf-8")
    except OSError:
        pass
    rc, err = _launchctl("kickstart", "-k", f"gui/{os.getuid()}/{SCHED_LABEL}")
    return rc == 0, err


def last_slot(freq: str, hour: int, minute: int, now: datetime) -> datetime:
    """Poslednji zakazani termin koji je već prošao (pre ili tačno sada)."""
    c = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
    if freq == "daily":
        if c > now:
            c -= timedelta(days=1)
    elif freq == "weekly":                       # ponedeljak
        c -= timedelta(days=c.weekday())
        if c > now:
            c -= timedelta(days=7)
    else:                                        # 1. u mesecu
        c = c.replace(day=1)
        if c > now:
            prev = c - timedelta(days=1)
            c = prev.replace(day=1)
    return c


def schedule_due(env: Env, now: Optional[datetime] = None) -> Tuple[bool, str]:
    """Da li treba napraviti backup: da, ako je prošao termin (posle uključivanja zakazivanja)
    za koji još nema uspešnog backupa — npr. zato što je Mac tada bio ugašen."""
    now = now or datetime.now()
    st = schedule_status(env)
    if not st or st.get("broken"):
        return True, "nema podataka o zakazivanju — pravim backup"
    slot = last_slot(st["freq"], st["hour"], st["minute"], now)
    try:
        installed = datetime.fromtimestamp(sched_paths(env)[0].stat().st_mtime)
    except OSError:
        installed = datetime.min
    if slot < installed:
        return False, "od uključivanja još nije bilo termina — preskačem"
    lr = load_json(last_run_path(env))
    if isinstance(lr, dict) and lr.get("ok") and lr.get("time"):
        last_ok = parse_ts(lr["time"])
        if last_ok and last_ok.replace(tzinfo=None) >= slot:
            return False, f"backup za termin {slot:%d.%m.%Y. %H:%M} je već napravljen — preskačem"
    return True, f"termin {slot:%d.%m.%Y. %H:%M} nema backup — pravim ga"


def rotate_backups(dest: Path, keep: int, current: Path) -> List[Path]:
    """Briše najstarije AUTOMATSKE backupe preko broja `keep`. Dira SAMO ClaudeBackup_*.zip fajlove čiji
    MANIFEST kaže da ih je napravio automatski backup ovog programa — ručne backupe i sve ostalo nikad,
    kao ni trenutni backup."""
    if keep < 1:
        return []
    auto = []
    try:
        names = sorted(p for p in dest.iterdir()
                       if BACKUP_ZIP_RE.match(p.name) and p.is_file() and not p.is_symlink()
                       and os.path.realpath(p) != os.path.realpath(current))
    except OSError:
        return []
    for p in names:
        try:
            with zipfile.ZipFile(p) as z:
                man = [n for n in z.namelist() if n.endswith("/MANIFEST.json") and n.count("/") == 1]
                if not man:
                    continue
                m = json.loads(z.read(man[0]).decode("utf-8"))
            if isinstance(m, dict) and m.get("format") == "claude-backup" and (m.get("scope") or {}).get("auto") is True:
                auto.append(p)
        except Exception:
            continue
    removed = []
    for p in auto[: max(0, len(auto) - (keep - 1))]:
        try:
            os.remove(p)   # samo stari automatski backup ovog programa
            removed.append(p)
        except OSError:
            continue
    return removed


def last_run_path(env: Env) -> Path:
    return sched_paths(env)[1].parent / "last_run.json"


def write_last_run(env: Env, ok: bool, message: str, path: Optional[Path] = None):
    p = last_run_path(env)
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        tmp = p.with_name(p.name + ".tmp")
        tmp.write_text(json.dumps({"time": datetime.now(timezone.utc).isoformat(), "ok": ok, "message": message,
                                   "path": str(path) if path else ""}, ensure_ascii=False), encoding="utf-8")
        os.replace(tmp, p)
    except OSError:
        pass


def notify(title: str, msg: str):
    if not IS_MAC:
        return
    script = f"display notification {json.dumps(msg, ensure_ascii=False)} with title {json.dumps(title, ensure_ascii=False)}"
    try:
        subprocess.run(["osascript", "-e", script], capture_output=True, timeout=15)
    except (OSError, subprocess.SubprocessError):
        pass


# ════════════════════════════════════════════════════════════ terminal UI

NO_COLOR = bool(os.environ.get("NO_COLOR"))


def _sgr(*c: str) -> str:
    return "" if NO_COLOR else "\x1b[" + ";".join(c) + "m"


ST = {
    "": "", "b": _sgr("1"), "dim": _sgr("38;5;245"), "acc": _sgr("38;5;209"), "accb": _sgr("1", "38;5;209"),
    "ok": _sgr("38;5;114"), "okb": _sgr("1", "38;5;114"), "warn": _sgr("38;5;221"), "err": _sgr("38;5;203"),
    "errb": _sgr("1", "38;5;203"), "white": _sgr("1", "38;5;255"),
    "hdr": _sgr("48;5;236", "38;5;250"), "hdracc": _sgr("48;5;236", "1", "38;5;209"),
    "sel": _sgr("48;5;237"), "btn": _sgr("48;5;209", "38;5;232", "1"), "btnoff": _sgr("38;5;209"),
}
RESET = "" if NO_COLOR else "\x1b[0m"


def cw(ch: str) -> int:
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def slen(s: str) -> int:
    return sum(cw(c) for c in s)


def clip(s: str, w: int) -> str:
    if slen(s) <= w:
        return s
    out, used = [], 0
    for ch in s:
        if used + cw(ch) > w - 1:
            break
        out.append(ch)
        used += cw(ch)
    return "".join(out) + "…"


def wrap(text: str, width: int) -> List[str]:
    out = []
    for para in text.split("\n"):
        out += textwrap.wrap(para, max(10, width)) or [""]
    return out


def render(segs: List[Tuple[str, str]], width: int, bg: str = "") -> str:
    out, used = [], 0
    for st, txt in segs:
        txt = nfc(txt)
        room = width - used
        if room <= 0:
            break
        if slen(txt) > room:
            txt = clip(txt, room)
        out.append(ST.get(st, st) + bg + txt + RESET)
        used += slen(txt)
    if bg and used < width:
        out.append(bg + " " * (width - used) + RESET)
    return "".join(out)


@dataclass
class Row:
    kind: str                 # item, option, button, text, head, gap
    label: str = ""
    key: str = ""
    right: str = ""
    desc: str = ""
    checked: bool = False
    disabled: bool = False
    locked: bool = False
    style: str = ""
    rstyle: str = ""

    @property
    def focusable(self) -> bool:
        return self.kind == "item" or (self.kind in ("option", "button") and not self.disabled)


class Key:
    __slots__ = ("name", "ch", "x", "y", "btn", "press")

    def __init__(self, name, ch="", x=0, y=0, btn=0, press=True):
        self.name, self.ch, self.x, self.y, self.btn, self.press = name, ch, x, y, btn, press


class UI:
    def __init__(self):
        self.fd = sys.stdin.fileno()
        self.buf = b""
        self.resized = False
        self.active = False
        self.old = None
        self.prev_winch = None
        self._last_prog = 0.0

    # ---- terminal
    def w(self, s: str):
        sys.stdout.write(s)
        sys.stdout.flush()

    def start(self):
        self.old = termios.tcgetattr(self.fd)
        new = termios.tcgetattr(self.fd)
        new[3] &= ~(termios.ICANON | termios.ECHO | termios.IEXTEN)
        new[6][termios.VMIN] = 1
        new[6][termios.VTIME] = 0
        termios.tcsetattr(self.fd, termios.TCSADRAIN, new)
        self.w("\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[?1000h\x1b[?1006h\x1b[2J")
        self.prev_winch = signal.signal(signal.SIGWINCH, self._winch)
        self.active = True

    def stop(self):
        if not self.active:
            return
        self.active = False
        self.w("\x1b[?1000l\x1b[?1006l\x1b[?7h\x1b[?25h\x1b[0m\x1b[?1049l")
        try:
            termios.tcsetattr(self.fd, termios.TCSADRAIN, self.old)
        except termios.error:
            pass
        signal.signal(signal.SIGWINCH, self.prev_winch or signal.SIG_DFL)

    def _winch(self, *_):
        self.resized = True

    def size(self) -> Tuple[int, int]:
        try:
            s = os.get_terminal_size(sys.stdout.fileno())
            return max(50, s.columns), max(16, s.lines)
        except OSError:
            return 80, 24

    def flush_input(self):
        try:
            termios.tcflush(self.fd, termios.TCIFLUSH)
        except termios.error:
            pass
        self.buf = b""

    def _more(self, t: float) -> bool:
        r, _, _ = select.select([self.fd], [], [], t)
        if r:
            data = os.read(self.fd, 4096)
            if data:
                self.buf += data
                return True
        return False

    def _parse(self) -> Optional[Key]:
        b = self.buf
        if not b:
            return None
        if b[0] == 0x1B:
            if len(b) == 1:
                if self._more(0.05):
                    return self._parse()
                self.buf = b""
                return Key("esc")
            if b.startswith(b"\x1b[<"):
                m = re.match(rb"\x1b\[<(\d+);(\d+);(\d+)([Mm])", b)
                if not m:
                    if len(b) < 40 and self._more(0.05):
                        return self._parse()
                    self.buf = b[1:]
                    return Key("esc")
                self.buf = b[m.end():]
                return Key("mouse", btn=int(m.group(1)), x=int(m.group(2)), y=int(m.group(3)), press=m.group(4) == b"M")
            if b.startswith(b"\x1b[M"):
                if len(b) < 6:
                    if self._more(0.05):
                        return self._parse()
                    self.buf = b""
                    return None
                code, x, y = b[3] - 32, b[4] - 32, b[5] - 32
                self.buf = b[6:]
                if code >= 64:
                    return Key("mouse", btn=code & 0x43, x=x, y=y, press=True)
                return Key("mouse", btn=code & 3, x=x, y=y, press=(code & 3) != 3)
            m = re.match(rb"\x1b\[([0-9;]*)([A-Za-z~])", b) or re.match(rb"\x1bO([0-9;]*)([A-Za-z])", b)
            if m:
                self.buf = b[m.end():]
                p, f = m.group(1).decode(), m.group(2).decode()
                if f == "~":
                    name = {"1": "home", "7": "home", "4": "end", "8": "end", "5": "pgup", "6": "pgdn",
                            "3": "delete"}.get(p.split(";")[0], "unknown")
                else:
                    name = {"A": "up", "B": "down", "C": "right", "D": "left", "H": "home", "F": "end",
                            "Z": "shifttab"}.get(f, "unknown")
                return Key(name)
            if re.fullmatch(rb"\x1b[\[O][0-9;]*", b) and self._more(0.05):
                return self._parse()
            self.buf = b[1:]
            return Key("esc")
        c = b[0]
        if c in (10, 13):
            self.buf = b[2:] if b[:2] == b"\r\n" else b[1:]
            return Key("enter")
        if c in (8, 127):
            self.buf = b[1:]
            return Key("backspace")
        if c == 9:
            self.buf = b[1:]
            return Key("tab")
        if c == 32:
            self.buf = b[1:]
            return Key("space")
        if c < 32:
            self.buf = b[1:]
            return Key("ctrl", ch=chr(c + 64))
        n = 1 if c < 0x80 else 2 if c >> 5 == 6 else 3 if c >> 4 == 14 else 4 if c >> 3 == 30 else 1
        if len(b) < n:
            if self._more(0.05):
                return self._parse()
            self.buf = b""
            return None
        self.buf = b[n:]
        return Key("char", ch=b[:n].decode("utf-8", "replace"))

    def read_key(self) -> Key:
        while True:
            if self.resized:
                self.resized = False
                return Key("resize")
            if self.buf:
                k = self._parse()
                if k:
                    return k
                continue
            r, _, _ = select.select([self.fd], [], [], 0.25)
            if r:
                data = os.read(self.fd, 4096)
                if not data:
                    raise EOFError
                self.buf += data

    def draw(self, lines: List[str]):
        _, H = self.size()
        lines = lines[:H]
        parts = ["\x1b[H"]
        for i, s in enumerate(lines):
            parts.append(s + "\x1b[K")
            if i < len(lines) - 1:
                parts.append("\r\n")
        parts.append("\x1b[J")
        self.w("".join(parts))

    # ---- delovi ekrana
    def header(self, W: int, step: Optional[str]) -> str:
        left = [("hdracc", " ◆ Claude Backup"), ("hdr", "   sigurna kopija Claude razgovora")]
        right = (step or "") + " "
        lw = sum(slen(t) for _, t in left)
        if lw + slen(right) + 2 > W:
            left = left[:1]
            lw = slen(left[0][1])
        pad = max(1, W - lw - slen(right))
        return render(left + [("hdr", " " * pad), ("hdr", right)], W)

    def top(self, W: int, title: str, step: Optional[str], intro=None, banner=None) -> List[str]:
        out = [self.header(W, step), "", render([("white", " " + title)], W)]
        for st, txt in intro or []:
            for wl in wrap(txt, W - 4):
                out.append(render([(st, "  " + wl)], W))
        if banner:
            out.append("")
            for st, txt in banner:
                for k, wl in enumerate(wrap(txt, W - 6)):
                    out.append(render([(st, "  ▌ " + wl)], W))
        out.append("")
        return out

    def layout(self, rows: List[Row], idxs: List[int], focused: int, W: int):
        body: List[Tuple[str, list]] = []
        at: Dict[int, int] = {}
        k = 0
        while k < len(idxs):
            i = idxs[k]
            r = rows[i]
            if r.kind == "button":
                segs, spans, x = [("", "   ")], [], 4
                while k < len(idxs) and rows[idxs[k]].kind == "button":
                    i = idxs[k]
                    b = rows[i]
                    lab = f"[ {b.label} ]"
                    if spans and x + slen(lab) > W - 1:      # ne staje u red → novi red dugmadi
                        body.append((render(segs, W), spans))
                        segs, spans, x = [("", "   ")], [], 4
                    st = "btn" if i == focused else ("dim" if b.disabled else "btnoff")
                    segs.append((st, lab))
                    spans.append((x, x + slen(lab) - 1, i))
                    at[i] = len(body)
                    x += slen(lab) + 3
                    segs.append(("", "   "))
                    k += 1
                body.append((render(segs, W), spans))
                continue
            if r.kind == "gap":
                at[i] = len(body)
                body.append(("", []))
            elif r.kind == "head":
                at[i] = len(body)
                body.append((render([("accb", "   " + r.label)], W), []))
            elif r.kind == "text":
                at[i] = len(body)
                for wl in wrap(r.label, W - 8):
                    body.append((render([(r.style, "     " + wl)], W), []))
            else:
                foc = i == focused
                bg = ST["sel"] if foc else ""
                segs = [("accb", " › ") if foc else ("", "   ")]
                if r.kind == "item":
                    if r.disabled:
                        segs.append(("dim", "[–] "))
                    elif r.checked or r.locked:
                        segs.append(("dim" if r.locked else "okb", "[✔] "))
                    else:
                        segs.append(("dim", "[ ] "))
                left_w = sum(slen(t) for _, t in segs)
                room = W - left_w - 2
                right = r.right
                if slen(right) > room // 2:
                    right = clip(right, max(6, room // 2))
                lab = clip(r.label, max(6, room - slen(right) - 2))
                pad = max(2, room - slen(lab) - slen(right))
                lab_st = "dim" if r.disabled else ("white" if foc else (r.style or ""))
                segs += [(lab_st, lab), ("", " " * pad), (r.rstyle or "dim", right)]
                at[i] = len(body)
                body.append((render(segs, W, bg), [(1, W, i)]))
            k += 1
        return body, at

    def screen(self, title: str, rows: List[Row], *, step: Optional[str] = None, intro=None, banner=None,
               mode: str = "check", hotkeys: Optional[Dict[str, str]] = None, focus: Optional[str] = None,
               on_toggle: Optional[Callable] = None, hints: Optional[str] = None):
        """Ekran sa listom. Dugmad sa kraja liste su uvek vidljiva na dnu ekrana."""
        foc = [i for i, r in enumerate(rows) if r.focusable]
        ci = 0
        if focus is not None:
            for j, i in enumerate(foc):
                if rows[i].key == focus:
                    ci = j
        end = len(rows)
        while end and rows[end - 1].kind == "gap":
            end -= 1
        bstart = end
        while bstart and rows[bstart - 1].kind == "button":
            bstart -= 1
        fixed = list(range(bstart, end))
        scroll_idx = list(range(bstart))
        while scroll_idx and rows[scroll_idx[-1]].kind == "gap":
            scroll_idx.pop()
        scroll, follow = 0, True
        if hints is None:
            hints = ("↑↓ biraj · Razmak/Enter označi · Tab → dugmad · Esc nazad · radi i miš"
                     if mode == "check" else "↑↓ biraj · Enter potvrdi · Esc nazad · radi i klik mišem")
        while True:
            W, H = self.size()
            out = self.top(W, title, step, intro, banner)
            fi = foc[ci] if foc else -1
            cur = rows[fi] if foc else None
            desc_lines = wrap(cur.desc, W - 6)[:3] if cur and cur.desc else []
            body, at = self.layout(rows, scroll_idx, fi, W)
            bbody, _ = self.layout(rows, fixed, fi, W) if fixed else ([], {})
            nd = 3 if H >= 30 else 2 if H >= 20 else 0
            bottom = ((1 + len(bbody)) if fixed else 0) + 1 + nd + 1
            avail = H - len(out) - bottom
            if avail < 3:
                out = out[:3] + [""]
                avail = max(1, H - len(out) - bottom)
            if follow and fi in at:
                fl = at[fi]
                if ci == 0:
                    scroll = 0
                if fl < scroll:
                    scroll = fl
                if fl >= scroll + avail:
                    scroll = fl - avail + 1
            scroll = max(0, min(scroll, max(0, len(body) - avail)))
            vis = body[scroll:scroll + avail]
            clicks = {}
            for s, spans in vis:
                if spans:
                    clicks[len(out) + 1] = spans
                out.append(s)
            out += [""] * (avail - len(vis))
            if fixed:
                out.append("")
                for line, spans in bbody:
                    clicks[len(out) + 1] = spans
                    out.append(line)
            more_up, more_dn = scroll > 0, scroll + avail < len(body)
            marker = (" ▲ ima još gore " if more_up else "") + (" ▼ ima još dole — skroluj " if more_dn else "")
            out.append(render([("dim", "─" * 3), ("acc", marker), ("dim", "─" * max(0, W - 3 - slen(marker)))], W))
            for k in range(nd):
                out.append(render([("", "  " + desc_lines[k])], W) if k < len(desc_lines) else "")
            out.append(render([("dim", "  " + hints)], W))
            self.draw(out)

            k = self.read_key()
            if k.name == "resize":
                continue
            if k.name == "mouse":
                if not k.press:
                    continue
                if k.btn in (64, 65):
                    scroll += -3 if k.btn == 64 else 3
                    follow = False
                elif k.btn == 0:
                    for x0, x1, idx in clicks.get(k.y, []):
                        if x0 <= k.x <= x1 and idx in foc:
                            ci, follow = foc.index(idx), True
                            res = self._act(rows, idx, on_toggle)
                            if res:
                                return res
                            break
                continue
            if k.name in ("esc", "backspace") or (k.name == "char" and k.ch.lower() == "q"):
                return ("back", None)
            if not foc:
                if k.name == "enter":
                    return ("back", None)
                continue
            if k.name == "up" or (k.name == "char" and k.ch == "k"):
                if ci > 0:
                    ci, follow = ci - 1, True
                else:
                    scroll, follow = scroll - 1, False
            elif k.name == "down" or (k.name == "char" and k.ch == "j"):
                if ci < len(foc) - 1:
                    ci, follow = ci + 1, True
                else:
                    scroll, follow = scroll + 1, False
            elif k.name in ("left", "right") and rows[fi].kind == "button":
                nj = ci + (-1 if k.name == "left" else 1)
                if 0 <= nj < len(foc) and rows[foc[nj]].kind == "button":
                    ci, follow = nj, True
            elif k.name == "pgup":
                scroll, follow = scroll - max(1, avail - 1), False
            elif k.name == "pgdn":
                scroll, follow = scroll + max(1, avail - 1), False
            elif k.name == "home":
                ci, follow = 0, True
            elif k.name == "end":
                ci, follow = len(foc) - 1, True
            elif k.name in ("tab", "shifttab"):
                btns = [j for j, i in enumerate(foc) if rows[i].kind == "button"]
                if btns:
                    after = [j for j in btns if j > ci]
                    ci = (after[0] if after else btns[0]) if k.name == "tab" else btns[0]
                    follow = True
            elif k.name in ("enter", "space"):
                res = self._act(rows, fi, on_toggle)
                if res:
                    return res
            elif k.name == "char":
                c = k.ch.lower()
                if hotkeys and c in hotkeys:
                    return ("hotkey", hotkeys[c])
                if c == "a" and mode == "check":
                    its = [r for r in rows if r.kind == "item" and not r.locked and not r.disabled]
                    val = not all(r.checked for r in its)
                    for r in its:
                        r.checked = val
                    if on_toggle:
                        on_toggle(rows, None)

    def _act(self, rows: List[Row], idx: int, on_toggle):
        r = rows[idx]
        if r.kind == "item":
            if not (r.locked or r.disabled):
                r.checked = not r.checked
                if on_toggle:
                    on_toggle(rows, r)
            return None
        if r.kind == "option":
            return ("select", r.key)
        if r.kind == "button":
            return ("button", r.key)
        return None

    def ask_text(self, title: str, prompt: str, initial: str = "", step=None, intro=None, banner=None) -> Optional[str]:
        val = initial
        self.w("\x1b[?1000l\x1b[?1006l")       # da bi prevlačenje foldera i lepljenje radili normalno
        try:
            while True:
                W, H = self.size()
                out = self.top(W, title, step, intro, banner)
                out.append(render([("", "  " + prompt)], W))
                out.append("")
                shown = val if slen(val) < W - 8 else "…" + val[-(W - 10):]
                out.append(render([("accb", "  › "), ("white", shown), ("acc", "█")], W))
                out += [""] * max(0, H - len(out) - 2)
                out.append(render([("dim", "─" * W)], W))
                out.append(render([("dim", "  Enter potvrdi · Esc nazad · folder možeš i da prevučeš iz Findera u ovaj prozor")], W))
                self.draw(out)
                k = self.read_key()
                if k.name == "enter":
                    return val
                if k.name == "esc":
                    return None
                if k.name == "backspace":
                    val = val[:-1]
                elif k.name == "space":
                    val += " "
                elif k.name == "char":
                    val += k.ch
        finally:
            self.w("\x1b[?1000h\x1b[?1006h")

    def progress(self, title: str, frac: float, line1: str, line2: str = "", force: bool = False):
        now = time.time()
        if not force and now - self._last_prog < 0.07 and frac < 1:
            return
        self._last_prog = now
        W, H = self.size()
        bw = max(10, min(54, W - 16))
        f = max(0.0, min(1.0, frac))
        filled = int(bw * f)
        out = self.top(W, title, None)
        out.append(render([("acc", "  " + "█" * filled), ("dim", "░" * (bw - filled)), ("white", f"  {int(f * 100):3d}%")], W))
        out.append("")
        out.append(render([("", "  " + line1)], W))
        out.append(render([("dim", "  " + line2)], W))
        out += [""] * max(0, H - len(out) - 1)
        out.append(render([("dim", "  Ne zatvaraj prozor. Ctrl+C prekida — ništa u Claude-u se ne menja.")], W))
        self.draw(out)


# ════════════════════════════════════════════════════════════ interaktivni program

HELP_ROWS = [
    ("head", "Šta program radi"),
    ("text", "• Pravi kopiju tvojih Claude Code razgovora, konteksta, podešavanja i skillova."),
    ("text", "• Samo ČITA Claude fajlove. Ništa u Claude-u ne briše i ne menja."),
    ("text", "• Svaki fajl kopira bajt po bajt i proverava ga SHA-256 kontrolnim zbirom, pa ceo backup još jednom proveri na kraju."),
    ("text", "• Vraćanje iz backupa samo dodaje fajlove koji fale. Tvoje postojeće fajlove ne prepisuje."),
    ("gap", ""),
    ("head", "Gde Claude čuva šta"),
    ("text", "• Razgovori: ~/.claude/projects/<projekat>/<id>.jsonl — jedan fajl po razgovoru (terminal i Code tab)."),
    ("text", "• Memorija projekta: ~/.claude/projects/<projekat>/memory — ono što Claude sam zapamti."),
    ("text", "• CLAUDE.md: uputstva koja Claude čita na početku razgovora (~/.claude/CLAUDE.md i u projektu)."),
    ("text", "• Skillovi, agenti, komande: ~/.claude/skills, agents, commands. Podešavanja: ~/.claude/settings.json."),
    ("text", "• ~/.claude.json: spisak projekata, MCP serveri, podešavanja naloga."),
    ("text", "• Code tab lista i Cowork: ~/Library/Application Support/Claude."),
    ("text", "• Slike iz razgovora i scratchpad: /private/tmp/claude-<broj> (macOS ih briše pri restartu)."),
    ("text", "• Chat tab: nije na Macu, nego na Anthropic serveru. Izvoz: claude.ai → Settings → Privacy → Export data."),
    ("gap", ""),
    ("head", "Čuvanje istorije (cleanupPeriodDays)"),
    ("text", "• Claude Code pri svakom pokretanju briše razgovore starije od ovog broja dana. Podrazumevano: 30."),
    ("text", "• Važi za SVE projekte. Briše razgovor, pod-agente, rezultate alata, checkpoint snimke (rewind), planove, nalepljen tekst i slike."),
    ("text", "• Ne briše memoriju projekta, CLAUDE.md, skillove, podešavanja ni history.jsonl."),
    ("text", "• Razgovore pokrenute ili nastavljene u Claude aplikaciji (Code tab, Cowork) Claude Code od verzije 2.1.248 čuva bez obzira na starost."),
    ("text", "• Obrisano ne može da se vrati — zato podesi 10 godina i pravi backup."),
    ("gap", ""),
    ("head", "Istorija jednog projekta"),
    ("text", f"• Čuva samo razgovore jednog projekta, direktno u njegov folder: <projekat>/{PROJECT_BACKUP_DIR}/."),
    ("text", "• Ako program pokreneš iz foldera projekta (cd moj-projekat && python3 …/claude_backup.py), "
             "prva stavka menija je „Sačuvaj istoriju ovog projekta”."),
    ("text", "• Bez menija: python3 claude_backup.py --project-backup (u folderu projekta)."),
    ("text", f"• U {PROJECT_BACKUP_DIR}/ ide .gitignore, pa istorija ne završi u git-u. Može da se vrati kao i svaki backup."),
    ("gap", ""),
    ("head", "Automatski backup"),
    ("text", "• Radi preko macOS planera (launchd): dnevno, nedeljno ili mesečno. Ako je Mac uspavan, pokrene se "
             "čim se probudi; ako je bio ugašen, backup se napravi čim ga upališ i uloguješ se."),
    ("text", "• Posle svakog backupa stiže notifikacija. Brišu se samo stari automatski ZIP-ovi preko broja koji izabereš, "
             "i to tek kad novi prođe proveru."),
    ("text", "• Ako su projekti u Documents ili na Desktopu, macOS iz pozadine možda neće pustiti čitanje njihovih "
             "CLAUDE.md fajlova — dodaj python3 u System Settings → Privacy & Security → Full Disk Access."),
]


class App:
    def __init__(self, ui: UI, env: Env):
        self.ui, self.env = ui, env
        self.inv: Optional[Inventory] = None
        self.ret: dict = {}
        self.here: Optional[Proj] = None
        self.exit_lines: List[Tuple[str, str]] = []

    def rescan(self):
        self.ui.progress("Skeniram tvoje Claude fajlove…", 0.0, "Samo čitam — ništa se ne menja.", "", force=True)
        self.inv = scan(self.env)
        self.ret = retention_info(self.env, self.inv.cc_version)
        try:
            self.here = project_for_path(self.inv, os.getcwd())
        except OSError:
            self.here = None

    def running_banner(self) -> Optional[list]:
        if self.inv and self.inv.running:
            return [("warn", "⚠ Claude je upaljen — zatvori ga (Cmd+Q) za najtačniji backup. R = proveri")]
        return None

    def run(self):
        self.rescan()
        while True:
            r = self.main_menu()
            if r == "backup":
                self.backup_wizard()
            elif r == "proj_here" and self.here:
                self.project_wizard(self.here.key)
            elif r == "proj":
                self.project_wizard(None)
            elif r == "restore":
                self.restore_wizard()
            elif r == "verify":
                self.verify_wizard()
            elif r == "retention":
                self.retention_wizard()
            elif r == "schedule":
                self.schedule_wizard()
            elif r == "launcher":
                ok, msg = make_launcher(self.env)
                self.message("Ikonica na Desktopu" if ok else "Ikonica nije napravljena", [("ok" if ok else "warn", msg)])
            elif r == "help":
                self.ui.screen("Pomoć: šta je šta", [Row(k, t) for k, t in HELP_ROWS] + [Row("gap"), Row("button", "← Nazad", "back")],
                               mode="select", focus="back")
            elif r == "rescan":
                self.rescan()
            else:
                return

    # ---- glavni meni
    def main_menu(self) -> str:
        inv, ret = self.inv, self.ret
        n_sess = sum(p.sessions for p in inv.projects.values())
        n_proj = sum(1 for p in inv.projects.values() if p.sessions)
        found = [f"{n_convs(n_sess)} u {plural(n_proj, 'projektu', 'projekta', 'projekata')}"]
        if inv.cats["cowork"].files:
            found.append(f"Cowork {human(inv.cats['cowork'].size)}")
        intro = [("", "Pronađeno: " + " · ".join(found))]
        sched = schedule_status(self.env) if IS_MAC else None
        if sched and not sched.get("broken"):
            intro.append(("ok", "✔ Automatski backup: " + describe_schedule(self.env, sched)))
        eff = ret["effective"]
        if eff >= RECOMMENDED_DAYS:
            intro.append(("ok", f"✔ Čuvanje istorije: {num(eff)} dana"))
        else:
            intro.append(("warn", f"⚠ Čuvanje istorije: {eff} dana — Claude Code briše starije razgovore."))
        rows = []
        if self.here:
            h = self.here
            rows.append(Row("option", f"Sačuvaj istoriju ovog projekta: {Path(h.cwd).name}", "proj_here",
                            right=f"{n_convs(h.sessions)} → {PROJECT_BACKUP_DIR}/", rstyle="ok",
                            desc=f"Samo razgovori projekta {self.env.tilde(h.cwd)} — čuvaju se ovde, u "
                                 f"{self.env.tilde(h.cwd)}/{PROJECT_BACKUP_DIR}/."))
        rows += [
            Row("option", "Napravi backup", "backup", right="razgovori, kontekst, podešavanja",
                desc="Pun backup: biraš šta čuvaš, u kom formatu i gde. Program samo čita Claude fajlove — ništa ne briše i ne menja."),
            Row("option", "Istorija jednog projekta", "proj", right=f"u njegov folder ({PROJECT_BACKUP_DIR}/)",
                desc="Biraš projekat; samo njegovi razgovori, memorija i slike se čuvaju direktno u folder tog projekta."),
            Row("option", "Vrati iz backupa", "restore", right="dodaje samo ono što fali",
                desc="Vraća fajlove iz backupa koji fale na ovom Macu. Postojeće fajlove nikad ne prepisuje."),
            Row("option", "Proveri backup", "verify", right="da li je ceo i ispravan",
                desc="Ponovo čita svaki fajl iz backupa i poredi SHA-256 zbir sa spiskom (MANIFEST)."),
            Row("option", "Podesi čuvanje istorije", "retention", right=f"sada: {num(eff)} dana",
                rstyle="ok" if eff >= RECOMMENDED_DAYS else "warn",
                desc="cleanupPeriodDays — koliko dana Claude Code čuva razgovore pre nego što ih sam obriše."),
            Row("option", "Automatski backup", "schedule",
                right=("uključen" if sched and not sched.get("broken") else "isključen"),
                rstyle="ok" if sched and not sched.get("broken") else "dim",
                desc=(describe_schedule(self.env, sched) if sched and not sched.get("broken")
                      else "Zakaži dnevni, nedeljni ili mesečni backup — radi sam u pozadini (macOS launchd).")),
            Row("option", "Napravi ikonicu na Desktopu", "launcher", right="pokretanje duplim klikom",
                desc="Pravi „Claude Backup.command” na Desktopu: dupli klik otvara ovaj program."),
            Row("option", "Pomoć: šta je šta", "help", desc="Gde Claude čuva razgovore, šta je cleanupPeriodDays i kako radi vraćanje."),
            Row("option", "Izlaz", "exit", desc="Zatvori program."),
        ]
        res = self.ui.screen("Šta želiš da uradiš?", rows, intro=intro, banner=self.running_banner(), mode="select",
                             hotkeys={"r": "rescan"})
        if res[0] == "select":
            return res[1]
        if res[0] == "hotkey":
            return "rescan"
        return "exit"

    def message(self, title: str, lines: List[Tuple[str, str]], step=None):
        rows = [Row("text", t, style=s) for s, t in lines] + [Row("gap"), Row("button", "U redu", "ok")]
        self.ui.screen(title, rows, step=step, mode="select", focus="ok")

    # ---- backup
    def backup_wizard(self):
        inv = self.inv
        sel_cats = {cid for cid, c in inv.cats.items() if c.files and c.default}
        projs_all = [p for p in inv.projects.values() if p.sessions or p.slug]
        sel_projs = {p.key for p in projs_all}
        formats = {"html", "md"}
        dest: Optional[Path] = None
        as_zip = True
        steps = ["cats", "projs", "format", "dest", "confirm"]
        si = 0
        err = None
        while 0 <= si < len(steps):
            name = steps[si]
            need_projs = len(projs_all) > 1 and bool(sel_cats & PROJECT_SCOPED)
            if name == "projs" and not need_projs:
                si += 1 if getattr(self, "_dir", 1) > 0 else -1
                continue
            vis = [s for s in steps if s != "projs" or need_projs]
            step = f"Korak {vis.index(name) + 1} od {len(vis)}"
            self._dir = 1
            if name == "cats":
                res = self.step_cats(sel_cats, step, err)
                err = None
                if res == "next":
                    if not sel_cats:
                        err = "Izaberi bar jednu stavku."
                        continue
                    si += 1
                elif res == "rescan":
                    self.rescan()
                    inv = self.inv
                else:
                    return
            elif name == "projs":
                res = self.step_projs(projs_all, sel_projs, step, err)
                err = None
                if res == "next":
                    if not sel_projs:
                        err = "Izaberi bar jedan projekat."
                        continue
                    si += 1
                else:
                    self._dir = -1
                    si -= 1
            elif name == "format":
                res = self.step_formats(formats, sel_cats, step)
                if res == "next":
                    si += 1
                else:
                    self._dir = -1
                    si -= 1
            elif name == "dest":
                res = self.step_dest(step)
                if res is None:
                    self._dir = -1
                    si -= 1
                else:
                    dest = res
                    si += 1
            elif name == "confirm":
                projects = None if sel_projs == {p.key for p in projs_all} else set(sel_projs)
                res, as_zip = self.step_confirm(sel_cats, projects, formats, dest, as_zip, step)
                if res == "start":
                    self.do_backup([c for c in inv.cats if c in sel_cats], projects, formats, dest, as_zip)
                    return
                if res == "rescan":
                    self.rescan()
                    inv = self.inv
                    continue
                if res == "cancel":
                    return
                self._dir = -1
                si -= 1

    def step_cats(self, sel: set, step: str, err) -> str:
        inv = self.inv
        rows = [Row("head", "Claude Code")]
        for cid in ("code_sessions", "tmp_sessions", "global", "extensions", "project_files"):
            c = inv.cats[cid]
            rows.append(Row("item", c.title, cid, right=cat_note(inv, cid), desc=c.desc,
                            checked=cid in sel and bool(c.files), disabled=not c.files))
        rows.append(Row("head", "Claude aplikacija"))
        for cid in ("desktop_code", "cowork", "desktop_config", "user_files"):
            c = inv.cats[cid]
            rows.append(Row("item", c.title, cid, right=cat_note(inv, cid), desc=c.desc,
                            checked=cid in sel and bool(c.files), disabled=not c.files))
        rows.append(Row("text", "Chat tab nije na Macu (na serveru je): Settings → Privacy → Export data", style="dim"))
        rows += [Row("button", "Dalje →", "next"), Row("button", "← Nazad", "back")]
        banner = (self.running_banner() or []) + ([("err", "✖ " + err)] if err else [])
        res = self.ui.screen("Šta želiš da sačuvaš? (sve je već označeno)", rows, step=step, banner=banner or None,
                             hotkeys={"r": "rescan"})
        for r in rows:
            if r.kind == "item":
                if r.checked:
                    sel.add(r.key)
                else:
                    sel.discard(r.key)
        if res[0] == "button" and res[1] == "next":
            return "next"
        if res[0] == "hotkey":
            return "rescan"
        return "back"

    def step_projs(self, projs: List[Proj], sel: set, step: str, err) -> str:
        env = self.env
        rows = [Row("item", "Svi projekti", "__all__", checked=len(sel) == len(projs), right=n_projs(len(projs)),
                    desc="Označi ili odznači sve projekte odjednom."), Row("gap")]
        for p in sorted(projs, key=lambda p: -p.last):
            right = (f"{n_convs(p.sessions)} · {human(p.size)} · {fmt_epoch(p.last)}" if p.sessions
                     else "bez razgovora, samo Claude fajlovi")
            rows.append(Row("item", env.tilde(p.cwd) if p.cwd else p.key, p.key, checked=p.key in sel, right=right,
                            desc=f"Folder: {p.cwd or 'nepoznat'}\nRazgovori: ~/.claude/projects/{p.key}"))
        rows += [Row("gap"), Row("button", "Dalje →", "next"), Row("button", "← Nazad", "back")]

        def on_toggle(rs, r):
            items = [x for x in rs if x.kind == "item" and x.key != "__all__"]
            if r is not None and r.key == "__all__":
                for x in items:
                    x.checked = r.checked
            else:
                rs[0].checked = all(x.checked for x in items)

        res = self.ui.screen("Koje projekte?", rows, step=step, on_toggle=on_toggle,
                             banner=[("err", "✖ " + err)] if err else None,
                             intro=[("dim", "Odnosi se na razgovore, slike i Claude fajlove projekata. "
                                            "Podešavanja i skillovi se uvek čuvaju u celini.")])
        sel.clear()
        sel.update(r.key for r in rows if r.kind == "item" and r.key != "__all__" and r.checked)
        return "next" if res == ("button", "next") else "back"

    def step_formats(self, formats: set, sel_cats: set, step: str) -> str:
        has_conv = bool(sel_cats & {"code_sessions", "cowork"})
        why = "" if has_conv else " Nema razgovora u izboru, pa ova opcija nije dostupna."
        rows = [
            Row("item", "Original — Claude format", "orig", checked=True, locked=True, right="uvek",
                desc="Tačna kopija fajlova, bajt po bajt. Iz ovoga se vraća u Claude. Ne može da se isključi."),
            Row("item", "HTML — pregled u browseru", "html", checked="html" in formats and has_conv, disabled=not has_conv,
                right="preporuka", rstyle="ok",
                desc="Pregled.html: svi razgovori sa pretragom, izgledaju kao chat. Otvara se duplim klikom." + why),
            Row("item", "Markdown (.md)", "md", checked="md" in formats and has_conv, disabled=not has_conv,
                desc="Čitljivi tekst fajlovi — za Obsidian, VS Code, GitHub ili da ih ubaciš u drugi AI." + why),
            Row("gap"), Row("button", "Dalje →", "next"), Row("button", "← Nazad", "back"),
        ]
        res = self.ui.screen("U kom formatu?", rows, step=step,
                             intro=[("dim", "Original se uvek čuva. Čitljive verzije su dodatak za lakše čitanje.")])
        if has_conv:
            formats.clear()
            formats.update(r.key for r in rows if r.kind == "item" and r.key in ("html", "md") and r.checked)
        return "next" if res == ("button", "next") else "back"

    def dest_ok(self, p: Path) -> Optional[str]:
        try:
            p = p.resolve()
        except OSError:
            pass
        if not p.is_dir():
            return "Taj folder ne postoji."
        if not os.access(p, os.W_OK):
            return "U taj folder ne može da se piše."
        for src in (self.env.claude, self.env.app, self.env.tmp):
            s = str(src)
            if str(p) == s or str(p).startswith(s + "/"):
                return "Backup ne sme da ide u Claude-ov folder — izaberi drugi."
        return None

    def step_dest(self, step: str) -> Optional[Path]:
        env = self.env
        cands = []
        for name, p in (("Desktop", env.home / "Desktop"), ("Documents", env.home / "Documents"),
                        ("Downloads", env.home / "Downloads")):
            if p.is_dir():
                cands.append((name, p, ""))
        try:
            for v in sorted(Path("/Volumes").iterdir()):
                if os.path.realpath(v) != "/" and v.is_dir():
                    cands.append((f"{v.name}", v, "spoljni disk"))
        except OSError:
            pass
        err = None
        while True:
            rows = []
            for name, p, kind in cands:
                try:
                    free = human(shutil.disk_usage(p).free)
                except OSError:
                    free = "?"
                rows.append(Row("option", name, str(p), right=f"{kind + ' · ' if kind else ''}slobodno {free}",
                                desc=env.tilde(p)))
            rows += [Row("option", "Druga lokacija…", "__other__", right="upiši ili prevuci folder",
                         desc="Upiši putanju, ili prevuci folder iz Findera u ovaj prozor."),
                     Row("gap"), Row("button", "← Nazad", "back")]
            res = self.ui.screen("Gde da sačuvam backup?", rows, step=step, mode="select",
                                 banner=[("err", "✖ " + err)] if err else None)
            err = None
            if res[0] != "select":
                return None
            if res[1] == "__other__":
                txt = self.ui.ask_text("Gde da sačuvam backup?", "Putanja foldera:", step=step)
                if not txt:
                    continue
                txt = txt.strip().strip("'\"")
                txt = re.sub(r"\\(.)", r"\1", txt)
                p = Path(os.path.expanduser(txt))
            else:
                p = Path(res[1])
            err = self.dest_ok(p)
            if not err:
                return p

    def step_confirm(self, sel_cats: set, projects: Optional[set], formats: set, dest: Path, as_zip: bool, step: str):
        inv, env = self.inv, self.env
        entries = select_entries(inv, [c for c in inv.cats if c in sel_cats], projects)
        raw = sum(e.size for e in entries)
        conv_bytes = sum(e.size for e in entries if e.src.name.endswith(".jsonl"))
        est = raw + conv_bytes * ((0.7 if "html" in formats else 0) + (0.5 if "md" in formats else 0))
        try:
            free = shutil.disk_usage(dest).free
        except OSError:
            free = 0
        fm = " + ".join(["Original"] + [x for x, k in (("HTML", "html"), ("Markdown", "md")) if k in formats])
        rows = [Row("text", f"Gde:  {env.tilde(dest)}/ClaudeBackup_…{'.zip' if as_zip else ''}"),
                Row("text", f"Format: {fm} · veličina do ~{human(est)} · slobodno {human(free)}"),
                Row("item", "Spakuj u jedan ZIP fajl", "zip", checked=as_zip, right="preporuka",
                    desc="ZIP je jedan fajl — lako se čuva na disku ili u cloudu. Bez ZIP-a dobijaš običan folder "
                         "koji odmah možeš da otvoriš."),
                Row("text", "✔ Bezbedno: Claude fajlove samo čitam. Ništa se ne briše i ne menja.", style="ok"),
                Row("head", "Šta čuvam")]
        for cid in inv.cats:
            if cid in sel_cats:
                rows.append(Row("text", f"✔ {CAT_TITLES[cid]} — {cat_note(inv, cid, projects)}", style="ok"))
        if projects is not None:
            rows.append(Row("text", f"  (samo {n_projs(len(projects))} od {n_projs(len([p for p in inv.projects.values() if p.sessions or p.slug]))})", style="dim"))
        rows += [Row("gap"),
                 Row("button", "Pokreni backup", "start"), Row("button", "← Nazad", "back"),
                 Row("button", "Odustani", "cancel")]
        banner = list(self.running_banner() or [])
        if free and est > free:
            banner.append(("err", f"✖ Možda nema dovoljno mesta: treba do ~{human(est)}, slobodno je {human(free)}."))
        res = self.ui.screen("Sve spremno — proveri pa pokreni", rows, step=step, banner=banner or None,
                             focus="start", hotkeys={"r": "rescan"})
        as_zip = next(r.checked for r in rows if r.key == "zip")
        if res[0] == "button":
            return res[1], as_zip
        if res[0] == "hotkey":
            return "rescan", as_zip
        return "back", as_zip

    def prog_cb(self, kind: str):
        titles = {"copy": "Kopiram i proveravam fajlove…", "read": "Pravim čitljivu verziju…",
                  "render": "Pravim čitljivu verziju…", "verify": "Proveravam gotov backup…",
                  "rescan": "Završna provera…", "plan": "Poredim backup sa ovim Macom…",
                  "restore": "Vraćam fajlove…"}

        def cb(phase, frac, a, b=""):
            self.ui.progress(titles.get(phase, kind), frac, a, b)
        return cb

    def do_backup(self, cats: List[str], projects: Optional[set], formats: set, dest: Path, as_zip: bool, **kw):
        try:
            res = run_backup(self.inv, cats, projects, formats, dest, as_zip, self.prog_cb("backup"), **kw)
        except KeyboardInterrupt:
            self.ui.flush_input()
            self.message("Prekinuto", [("warn", "Backup je prekinut i nedovršeni fajl je uklonjen. "
                                                "U Claude-u ništa nije promenjeno.")])
            return
        except OSError as e:
            self.ui.flush_input()
            self.message("Backup nije uspeo", [("err", f"✖ {os_err(e)}"), ("", "U Claude-u ništa nije promenjeno.")])
            return
        self.ui.flush_input()
        self.show_result(res)

    def show_result(self, res: BackupResult):
        lines = backup_report_lines(self.env, res, self.ret)
        self.exit_lines = lines
        title = ("✔ Backup je gotov i proveren" if res.verify.passed and not res.failed
                 else "⚠ Backup je gotov — pogledaj upozorenja" if res.verify.passed
                 else "✖ Provera backupa nije prošla")
        rows = [Row("text", t, style=s) if s != "head" else Row("head", t) for s, t in lines]
        rows.append(Row("gap"))
        pregled = res.path / "Pregled.html" if res.path.is_dir() else None
        if IS_MAC and pregled and pregled.is_file():
            rows.append(Row("button", "Otvori pregled", "open"))
        if IS_MAC:
            rows.append(Row("button", "Prikaži u Finderu", "finder"))
        if self.ret["effective"] < RECOMMENDED_DAYS:
            rows.append(Row("button", "Podesi čuvanje", "retention"))
        rows += [Row("button", "Glavni meni", "menu")]
        focus = "open" if IS_MAC and pregled and pregled.is_file() else "menu"
        while True:
            r = self.ui.screen(title, rows, mode="select", focus=focus)
            if r == ("button", "open"):
                subprocess.run(["open", str(pregled)], check=False)
                focus = "menu"
                continue
            if r == ("button", "finder"):
                subprocess.run(["open", "-R", str(res.path)], check=False)
                focus = "menu"
                continue
            if r == ("button", "retention"):
                self.retention_wizard()
            break

    # ---- istorija jednog projekta
    def project_wizard(self, key: Optional[str]):
        env = self.env
        picked_here = key is not None
        formats = {"html", "md"}
        as_zip = False
        step = 1 if key is None else 2
        while True:
            inv = self.inv
            if step == 1:
                projs = sorted([p for p in inv.projects.values() if p.sessions and p.cwd], key=lambda p: -p.last)
                if not projs:
                    self.message("Istorija jednog projekta", [("warn", "Nisam našao nijedan projekat sa razgovorima.")])
                    return
                rows = []
                for p in projs:
                    ok = os.path.isdir(p.cwd)
                    rows.append(Row("option", env.tilde(p.cwd), p.key, disabled=not ok,
                                    right=f"{n_convs(p.sessions)} · {fmt_epoch(p.last)}" if ok else "folder više ne postoji",
                                    desc=(f"Istorija ide u {env.tilde(p.cwd)}/{PROJECT_BACKUP_DIR}/" if ok else
                                          "Folder ovog projekta ne postoji na ovom Macu — njegove razgovore čuva „Napravi backup”.")))
                rows += [Row("gap"), Row("button", "← Nazad", "back")]
                res = self.ui.screen("Čiju istoriju da sačuvam?", rows, step="Korak 1 od 3", mode="select",
                                     intro=[("dim", f"Istorija se čuva direktno u folder tog projekta, u {PROJECT_BACKUP_DIR}/.")])
                if res[0] != "select":
                    return
                key = res[1]
                step = 2
                continue
            pr = inv.projects.get(key)
            if not pr:
                return
            if step == 2:
                r = self.step_formats(formats, {"code_sessions"}, "Korak 2 od 3")
                if r != "next":
                    if picked_here:
                        return
                    step = 1
                    continue
                step = 3
                continue
            # korak 3: potvrda
            entries = project_entries(inv, key)
            by = {}
            for f in entries:
                by.setdefault(f.cat, []).append(f)
            mem = [f for f in by.get("code_sessions", []) if "/memory/" in f.arc]
            per_sess = [f for f in by.get("global", [])]
            dest = Path(pr.cwd) / PROJECT_BACKUP_DIR
            prompts = project_prompt_history(env, pr.cwd)
            fm = " + ".join(["Original"] + [x for x, k in (("HTML", "html"), ("Markdown", "md")) if k in formats])
            rows = [Row("text", f"Gde:  {env.tilde(dest)}/"),
                    Row("text", f"Format: {fm}"),
                    Row("item", "Spakuj u jedan ZIP fajl", "zip", checked=as_zip,
                        desc="Bez ZIP-a dobijaš folder sa Pregled.html koji odmah otvaraš. Sa ZIP-om — jedan fajl."),
                    Row("text", "✔ Bezbedno: Claude fajlove samo čitam. Ništa se ne briše i ne menja. "
                                f"U {PROJECT_BACKUP_DIR}/ ide .gitignore, da istorija ne završi u git-u.", style="ok"),
                    Row("head", "Šta čuvam"),
                    Row("text", f"✔ {n_convs(pr.sessions)} — sa pod-agentima i rezultatima alata "
                                f"({human(sum(f.size for f in by.get('code_sessions', [])))})", style="ok")]
            if mem:
                rows.append(Row("text", f"✔ Claude-ova memorija ovog projekta ({n_files(len(mem))})", style="ok"))
            if by.get("tmp_sessions"):
                rows.append(Row("text", f"✔ Slike i scratchpad iz razgovora ({n_files(len(by['tmp_sessions']))})", style="ok"))
            if per_sess:
                rows.append(Row("text", f"✔ Checkpoint snimci (rewind) i zadaci tih razgovora ({n_files(len(per_sess))})", style="ok"))
            if by.get("desktop_code"):
                rows.append(Row("text", f"✔ Kartice iz Code taba ({n_files(len(by['desktop_code']))})", style="ok"))
            if by.get("project_files"):
                rows.append(Row("text", "✔ CLAUDE.md, .claude/ i .mcp.json iz projekta", style="ok"))
            if prompts:
                n_prompts = prompts.count(b"\n")
                rows.append(Row("text", f"✔ Poruke koje si kucao u ovom projektu ({num(n_prompts)})", style="ok"))
            rows += [Row("gap"),
                     Row("button", "Sačuvaj istoriju", "go"), Row("button", "← Nazad", "back"),
                     Row("button", "Odustani", "cancel")]
            res = self.ui.screen(f"Istorija projekta {Path(pr.cwd).name}", rows, step="Korak 3 od 3",
                                 banner=self.running_banner(), focus="go", hotkeys={"r": "rescan"})
            as_zip = next(r.checked for r in rows if r.key == "zip")
            if res == ("hotkey", "rescan"):
                self.rescan()
                continue
            if res == ("button", "go"):
                break
            if res == ("button", "cancel"):
                return
            step = 2
        err = prepare_project_dest(dest)
        if err:
            self.message("Ne mogu da pišem u folder projekta", [("err", f"✖ {env.tilde(dest)}: {err}"),
                                                                ("", "U Claude-u ništa nije promenjeno.")])
            return
        self.do_backup([], {key}, formats, dest, as_zip,
                       selector=lambda i, k=key: project_entries(i, k),
                       prefix=f"ClaudeBackup_{slug(Path(pr.cwd).name, 30)}",
                       scope={"type": "project", "cwd": pr.cwd, "key": key},
                       extra=(lambda gen, d=prompts: gen("kucane-poruke.jsonl", d)) if prompts else None)

    # ---- provera
    def pick_backup(self, title: str, step=None) -> Optional[Path]:
        env = self.env
        err = None
        extra = [Path.cwd(), Path.cwd() / PROJECT_BACKUP_DIR]
        sched = schedule_status(env) if IS_MAC else None
        if sched and sched.get("dest"):
            extra.append(Path(sched["dest"]))
        if self.inv:
            extra += [Path(p.cwd) / PROJECT_BACKUP_DIR for p in self.inv.projects.values() if p.cwd and p.slug]
        while True:
            found = find_backups(env, extra)
            rows = []
            for p in found[:40]:
                try:
                    size = human(p.stat().st_size) if p.is_file() else "folder"
                except OSError:
                    size = "?"
                rows.append(Row("option", p.name, str(p), right=f"{size} · {env.tilde(p.parent)}", desc=env.tilde(p)))
            if not found:
                rows.append(Row("text", "Nisam našao backup na Desktopu, u Documents, Downloads, ~/ClaudeBackups, "
                                        "u claude-istorija folderima projekata ni na spoljnim diskovima.", style="dim"))
            rows += [Row("option", "Druga lokacija…", "__other__", desc="Upiši putanju ZIP-a ili foldera, ili ga prevuci iz Findera."),
                     Row("gap"), Row("button", "← Nazad", "back")]
            res = self.ui.screen(title, rows, step=step, mode="select", banner=[("err", "✖ " + err)] if err else None)
            err = None
            if res[0] != "select":
                return None
            if res[1] == "__other__":
                txt = self.ui.ask_text(title, "Putanja backupa (ZIP ili folder):", step=step)
                if not txt:
                    continue
                txt = re.sub(r"\\(.)", r"\1", txt.strip().strip("'\""))
                p = Path(os.path.expanduser(txt))
                if not p.exists():
                    err = "Ne postoji: " + txt
                    continue
                return p
            return Path(res[1])

    def verify_wizard(self):
        p = self.pick_backup("Koji backup da proverim?")
        if not p:
            return
        vr = verify_backup(p, self.prog_cb("verify"))
        self.ui.flush_input()
        self.message(*verify_message(self.env, p, vr))

    # ---- vraćanje
    def restore_wizard(self):
        env = self.env
        p = self.pick_backup("Iz kog backupa da vratim?", step="Korak 1 od 3")
        if not p:
            return
        vr = verify_backup(p, self.prog_cb("verify"))
        if not vr.passed:
            self.ui.flush_input()
            t, lines = verify_message(env, p, vr)
            self.message("Ovaj backup ne vraćam", lines + [("err", "Vraćanje je otkazano da ne bi ubacio oštećene fajlove.")])
            return
        items = plan_restore(env, vr.manifest, self.prog_cb("plan"))
        self.ui.flush_input()
        by: Dict[str, Dict[str, int]] = {}
        for it in items:
            by.setdefault(it.e.get("cat", "?"), {}).setdefault(it.status, 0)
            by[it.e.get("cat", "?")][it.status] += 1
        rows = []
        for cid in [c for c in CAT_TITLES if c in by]:
            s = by[cid]
            add = s.get("add", 0)
            parts = [f"{num(add)} za vraćanje"] + ([f"{num(s['same'])} isto"] if s.get("same") else []) + \
                    ([f"{num(s['diff'])} drugačije (ostaje tvoje)"] if s.get("diff") else []) + \
                    ([f"{num(s['noproj'])} bez projekta"] if s.get("noproj") else [])
            rows.append(Row("item", CAT_TITLES[cid], cid, checked=add > 0 and cid != "tmp_sessions", disabled=add == 0,
                            right=" · ".join(parts),
                            desc="„isto” = već imaš identičan fajl. „drugačije” = imaš svoju verziju — ona ostaje, "
                                 "ne prepisujem je. „bez projekta” = folder projekta ne postoji na ovom Macu."))
        rows += [Row("gap"), Row("button", "Dalje →", "next"), Row("button", "← Nazad", "back")]
        man = vr.manifest
        intro = [("ok", f"✔ Backup je ispravan: {n_files(vr.ok)} proveren."),
                 ("dim", f"Napravljen {fmt_ts(man.get('created', ''))} na „{man.get('hostname', '?')}”.")]
        res = self.ui.screen("Šta da vratim?", rows, step="Korak 2 od 3", intro=intro)
        if res != ("button", "next"):
            return
        cats = {r.key for r in rows if r.kind == "item" and r.checked}
        chosen = [it for it in items if it.status == "add" and it.e.get("cat") in cats]
        if not chosen:
            self.message("Nema šta da se vrati", [("ok", "Sve iz izabranih stavki već postoji na ovom Macu.")])
            return
        set_ret = self.ret["effective"] < RECOMMENDED_DAYS
        while True:
            rows = [Row("text", f"Vraćam {n_files(len(chosen))} ({human(sum(int(i.e.get('size', 0)) for i in chosen))}) koji fale na ovom Macu."),
                    Row("text", "✔ Postojeće fajlove ne diram i ništa ne brišem.", style="ok"),
                    Row("gap")]
            if self.ret["effective"] < RECOMMENDED_DAYS:
                rows.append(Row("item", f"Prvo podesi čuvanje istorije na {num(RECOMMENDED_DAYS)} dana", "ret", checked=set_ret,
                                right="preporuka", rstyle="ok",
                                desc=f"Sada je {self.ret['effective']} dana. Bez ovoga Claude Code može odmah da obriše "
                                     "vraćene razgovore starije od toga."))
                rows.append(Row("gap"))
            rows += [Row("button", "Vrati", "go"), Row("button", "← Nazad", "back")]
            res = self.ui.screen("Potvrdi vraćanje", rows, step="Korak 3 od 3", banner=self.running_banner(),
                                 focus="go", hotkeys={"r": "rescan"})
            set_ret = any(r.checked for r in rows if r.key == "ret")
            if res == ("hotkey", "rescan"):
                self.inv.running = detect_running()
                continue
            break
        if res != ("button", "go"):
            return
        lines: List[Tuple[str, str]] = []
        rd = Reader(p)
        try:
            added, failed = apply_restore(rd, chosen, self.prog_cb("restore"))
        except KeyboardInterrupt:
            added, failed = sum(1 for i in chosen if i.target and i.target.exists()), [("—", "prekinuto")]
        finally:
            rd.close()
        self.ui.flush_input()
        lines.append(("ok", f"✔ Vraćeno: {n_files(added)}, svaki proveren SHA-256 zbirom."))
        # tek POSLE vraćanja: tako se cleanupPeriodDays dopisuje u tvoj vraćeni settings.json, umesto da novi
        # settings.json spreči vraćanje starog (sa tvojim modelom, dozvolama…)
        if set_ret:
            ok, msg, bak, ch = set_retention(env, RECOMMENDED_DAYS)
            lines.append(("ok" if ok else "err", ("✔ Čuvanje istorije: " if ok else "✖ Čuvanje istorije: ") + msg))
            self.ret = retention_info(env, self.inv.cc_version)
        skipped = sum(1 for i in chosen if i.status == "diff")
        if skipped:
            lines.append(("warn", f"⚠ {n_files(skipped)} se u međuvremenu pojavio — nije diran."))
        for src, why in failed[:8]:
            lines.append(("err", f"✖ {src} — {why}"))
        lines += [("", ""), ("", "Otvori Claude. Ako se neki razgovor ne vidi u Code tabu, nastavi ga iz terminala: "
                               "cd <folder projekta> && claude --resume")]
        self.exit_lines = lines
        self.rescan()
        self.message("Vraćanje je završeno" if not failed else "Vraćanje je završeno uz greške", lines)

    # ---- automatski backup
    def schedule_wizard(self):
        env = self.env
        if not IS_MAC:
            self.message("Automatski backup", [("warn", "Zakazivanje preko macOS planera (launchd) radi samo na Macu.")])
            return
        title = "Automatski backup"
        st = schedule_status(env)
        intro = []
        if st and not st.get("broken"):
            intro.append(("ok", "✔ Uključeno: " + describe_schedule(env, st)))
            if not st["loaded"]:
                intro.append(("warn", "⚠ macOS ga trenutno nije učitao. Izaberi učestalost ponovo da ga uključiš."))
            lr = load_json(last_run_path(env))
            if isinstance(lr, dict) and lr.get("time"):
                intro.append(("ok" if lr.get("ok") else "err",
                              f"{'✔' if lr.get('ok') else '✖'} Poslednji put {fmt_ts(lr['time'])}: {lr.get('message', '')}"))
            else:
                intro.append(("dim", "Još nije pokrenut — izaberi „Pokreni ga sada (probno)” da proveriš da radi."))
        else:
            intro.append(("dim", "Sada: isključeno. Izaberi koliko često da se pravi backup."))
        rows = [Row("option", "Svakog dana", "daily", desc="Jedan backup dnevno u vreme koje izabereš."),
                Row("option", "Jednom nedeljno", "weekly", right="ponedeljkom", desc="Svakog ponedeljka u vreme koje izabereš."),
                Row("option", "Jednom mesečno", "monthly", right="1. u mesecu", desc="Prvog dana u mesecu u vreme koje izabereš.")]
        if st:
            rows += [Row("gap"),
                     Row("option", "Pokreni ga sada (probno)", "run_now",
                         desc="Pokreće zakazani backup odmah, kroz macOS planer, kao da je stiglo vreme. "
                              "Dobićeš notifikaciju kad završi."),
                     Row("option", "Isključi automatski backup", "off", rstyle="warn", right="backupi ostaju",
                         desc="Uklanja samo zakazivanje. Backupi koji su već napravljeni ostaju.")]
        rows += [Row("gap"), Row("button", "← Nazad", "back")]
        res = self.ui.screen(title, rows, intro=intro, mode="select", step="Korak 1 od 4" if not st else None)
        if res[0] != "select":
            return
        if res[1] == "off":
            ok, msg = remove_schedule(env)
            self.message(title, [("ok" if ok else "err", ("✔ " if ok else "✖ ") + msg)])
            return
        if res[1] == "run_now":
            ok, err = run_schedule_now(env)
            _, _, log = sched_paths(env)
            self.message(title, [("ok", "✔ Pokrenuto u pozadini. Kad završi, stići će notifikacija.") if ok
                                 else ("err", f"✖ Nije pokrenuto: {err}"),
                                 ("dim", f"Dnevnik rada: {env.tilde(log)}")])
            return
        freq = res[1]

        # vreme
        rows = [Row("option", "03:00 — noću", "03:00", right="preporuka", rstyle="ok",
                    desc="Ako je Mac tada uspavan (zatvoren laptop), backup se pokrene čim ga probudiš."),
                Row("option", "09:00", "09:00"), Row("option", "13:00", "13:00"), Row("option", "20:00", "20:00"),
                Row("option", "Drugo vreme…", "other", desc="Upiši vreme u obliku SS:MM, npr. 22:30."),
                Row("gap"), Row("button", "← Nazad", "back")]
        res = self.ui.screen("U koliko sati?", rows, step="Korak 2 od 4", mode="select",
                             intro=[("dim", f"Učestalost: {FREQ_TEXT[freq]}. Ako je Mac uspavan, backup se pokrene kad se "
                                            "probudi. Ako je bio ugašen, napravi se čim ga upališ.")])
        if res[0] != "select":
            return
        hhmm = res[1]
        while hhmm == "other":
            txt = self.ui.ask_text("U koliko sati?", "Vreme (SS:MM):", "21:00", step="Korak 2 od 4")
            if txt is None:
                return
            m = re.fullmatch(r"\s*(\d{1,2})[:.](\d{2})\s*", txt)
            if m and int(m.group(1)) < 24 and int(m.group(2)) < 60:
                hhmm = f"{int(m.group(1)):02d}:{m.group(2)}"
        hour, minute = int(hhmm[:2]), int(hhmm[3:])

        # koliko čuvati
        rows = [Row("option", "Čuvaj poslednjih 7", "7"), Row("option", "Čuvaj poslednjih 14", "14", right="preporuka", rstyle="ok"),
                Row("option", "Čuvaj poslednjih 30", "30"),
                Row("option", "Čuvaj sve — ništa ne briši", "0",
                    desc="Backupi se gomilaju. Povremeno obriši stare ručno da ne zauzmu ceo disk.")]
        for r in rows[:3]:
            r.desc = (f"Kad novi backup prođe proveru, brišu se najstariji automatski backupi preko {r.key}. "
                      "Ručne backupe i ostale fajlove u tom folderu ne diram.")
        rows += [Row("gap"), Row("button", "← Nazad", "back")]
        res = self.ui.screen("Koliko starih backupa da čuvam?", rows, step="Korak 3 od 4", mode="select", focus="14")
        if res[0] != "select":
            return
        keep = int(res[1])

        # gde
        default = env.home / "ClaudeBackups"
        cands = [("~/ClaudeBackups", default, "preporuka")]
        try:
            for v in sorted(Path("/Volumes").iterdir()):
                if os.path.realpath(v) != "/" and v.is_dir():
                    cands.append((v.name + "/ClaudeBackups", v / "ClaudeBackups", "spoljni disk — mora biti prikačen"))
        except OSError:
            pass
        err = None
        while True:
            rows = [Row("option", lab, str(p), right=note, rstyle="ok" if note == "preporuka" else "dim",
                        desc=f"{env.tilde(p)} — macOS dozvoljava pozadinskim programima da pišu ovde."
                        if p == default else f"{p} — ako disk nije prikačen u to vreme, dobićeš notifikaciju.")
                    for lab, p, note in cands]
            rows += [Row("option", "Druga lokacija…", "__other__",
                         desc="Izbegavaj Desktop, Documents i Downloads — macOS tamo ne pušta pozadinske programe."),
                     Row("gap"), Row("button", "← Nazad", "back")]
            res = self.ui.screen("Gde da čuvam automatske backupe?", rows, step="Korak 4 od 4", mode="select",
                                 banner=[("err", "✖ " + err)] if err else None)
            err = None
            if res[0] != "select":
                return
            if res[1] == "__other__":
                txt = self.ui.ask_text("Gde da čuvam automatske backupe?", "Putanja foldera:", step="Korak 4 od 4")
                if not txt:
                    continue
                dest = Path(os.path.expanduser(re.sub(r"\\(.)", r"\1", txt.strip().strip("'\""))))
            else:
                dest = Path(res[1])
            parent_ok = dest.is_dir() or dest.parent.is_dir()
            err = None if parent_ok else "Taj folder (ni folder iznad njega) ne postoji."
            if not err:
                for src in (env.claude, env.app, env.tmp):
                    if str(dest) == str(src) or str(dest).startswith(str(src) + "/"):
                        err = "Backup ne sme da ide u Claude-ov folder — izaberi drugi."
            if not err:
                break

        when = f"{FREQ_TEXT[freq]} u {hour:02d}:{minute:02d}"
        rows = [Row("text", f"Kada: {when}"),
                Row("text", f"Gde: {env.tilde(dest)}  (ZIP: original + HTML pregled)"),
                Row("text", "Čuva: " + ("sve backupe" if not keep else
                                        plural(keep, "poslednji", "poslednja", "poslednjih") + "; stariji automatski "
                                        "backupi se brišu tek kad novi prođe proveru (ručne backupe ne diram)")),
                Row("text", "Posle svakog backupa stiže macOS notifikacija — i ako nešto ne uspe."),
                Row("text", "Ne moraš ništa da pališ: ni Terminal ni Claude. Ako je Mac u to vreme uspavan, backup se "
                            "napravi kad ga probudiš; ako je bio ugašen — čim ga upališ i uloguješ se."),
                Row("gap"),
                Row("text", "Kopija ovog programa ide u ~/Library/Application Support/ClaudeBackupTool, a zakazivanje u "
                            "~/Library/LaunchAgents. Tako radi i ako pomeriš ili obrišeš ovaj fajl.", style="dim"),
                Row("text", "Ako su ti projekti u Documents ili na Desktopu, macOS iz pozadine može da preskoči njihove "
                            "CLAUDE.md fajlove dok python3 ne dobije Full Disk Access. Razgovori i podešavanja iz "
                            "~/.claude prolaze bez toga, a izveštaj tačno kaže šta je preskočeno.", style="dim"),
                Row("gap"), Row("button", "Uključi automatski backup", "go"), Row("button", "← Nazad", "back")]
        res = self.ui.screen("Proveri pa uključi", rows, mode="select", focus="go")
        if res != ("button", "go"):
            return
        ok, msg = install_schedule(env, freq, hour, minute, dest, keep)
        if not ok:
            self.message(title, [("err", "✖ " + msg)])
            return
        rows = [Row("text", f"✔ Automatski backup je uključen: {when} → {env.tilde(dest)}.", style="ok"),
                Row("text", "Preporuka: pokreni ga sada jednom probno. Radi u pozadini, kao da je stiglo vreme, "
                            "i za minut-dva stiže notifikacija sa rezultatom.", style="dim"),
                Row("gap"), Row("button", "Pokreni probno sada", "run"), Row("button", "U redu", "ok")]
        res = self.ui.screen(title, rows, mode="select", focus="run")
        if res == ("button", "run"):
            ok, err = run_schedule_now(env)
            _, _, log = sched_paths(env)
            self.message(title, [("ok", "✔ Pokrenuto u pozadini. Kad završi, stići će notifikacija, a rezultat "
                                        "vidiš i ovde, u „Automatski backup”.") if ok
                                 else ("err", f"✖ Nije pokrenuto: {err}"),
                                 ("dim", f"Dnevnik rada: {env.tilde(log)}")])

    # ---- čuvanje istorije
    def retention_wizard(self):
        env = self.env
        self.ret = retention_info(env, self.inv.cc_version)
        r = self.ret
        cur = r["cleanup"]
        intro = []
        if r["error"]:
            intro.append(("err", "✖ " + r["error"]))
        intro.append(("", f"Sada: cleanupPeriodDays = {cur if cur is not None else 'nije podešeno (= 30 dana)'}"))
        if r["managed"] is not None:
            intro.append(("warn", f"⚠ Tvoja organizacija je podesila {r['managed']} dana — to ima prednost nad tvojim izborom."))
        if r["desktop_kept"]:
            intro.append(("ok", f"✔ Razgovori iz Code taba i Coworka se čuvaju bez obzira na starost (Claude Code {r['version']})."))
        elif r["desktop"] is not None:
            intro.append(("warn", f"⚠ desktopSessionCleanupPeriodDays = {r['desktop']} — i Code tab razgovori imaju rok."))
        elif r["version_known"]:
            intro.append(("warn", f"⚠ Tvoj Claude Code {r['version']} briše i Code tab razgovore po istom pravilu (zaštita je od 2.1.248)."))
        rows = [Row("text", "Claude Code pri svakom pokretanju briše razgovore starije od ovog broja dana — u SVIM "
                            "projektima, ne samo u onima koje ne koristiš. Briše razgovor, pod-agente, rezultate alata, "
                            "checkpoint snimke (rewind), planove i slike. Memoriju projekta, CLAUDE.md, skillove i "
                            "podešavanja ne briše. Obrisano ne može da se vrati.", style="dim"),
                Row("gap")]
        opts = [(3650, "10 godina (3.650 dana)", "preporuka"), (365, "1 godina (365 dana)", ""),
                (90, "3 meseca (90 dana)", ""), (30, "30 dana", "Claude podrazumevano")]
        for d, lab, right in opts:
            mark = "  ← sada" if (cur == d or (cur is None and d == 30)) else ""
            rows.append(Row("option", lab, str(d), right=(right + mark).strip(), rstyle="ok" if d == 3650 else "dim",
                            disabled=bool(r["error"]),
                            desc=f'U ~/.claude/settings.json upisujem "cleanupPeriodDays": {d}. Pre izmene pravim kopiju fajla.'))
        rows += [Row("gap"), Row("button", "← Nazad, ne menjaj ništa", "back")]
        res = self.ui.screen("Koliko dugo da Claude Code čuva razgovore?", rows, intro=intro, mode="select",
                             focus="3650" if not r["error"] else "back")
        if res[0] != "select":
            return
        days = int(res[1])
        ok, msg, bak, changes = set_retention(env, days)
        self.ret = retention_info(env, self.inv.cc_version)
        lines = [("ok" if ok else "err", ("✔ " if ok else "✖ ") + msg[0].upper() + msg[1:] + ".")]
        for ch in changes:
            lines.append(("", f"  ~/.claude/settings.json:  {ch}"))
        if bak:
            lines.append(("dim", f"  Stari fajl je sačuvan kao {env.tilde(bak)}"))
        if ok and changes:
            lines.append(("", "Važi od sledećeg pokretanja Claude Code-a. Ostatak fajla je ostao isti."))
        self.message("Čuvanje istorije", lines)


def verify_message(env: Env, p: Path, vr: VerifyResult) -> Tuple[str, List[Tuple[str, str]]]:
    if vr.error:
        return "✖ Backup ne može da se pročita", [("err", f"✖ {env.tilde(p)}"), ("err", vr.error)]
    if vr.passed:
        m = vr.manifest or {}
        return "✔ Backup je ispravan", [
            ("ok", f"✔ Svih {n_files(vr.ok)} ({human(vr.bytes)}) je identično onome što je sačuvano (SHA-256)."),
            ("", f"Backup: {env.tilde(p)}"),
            ("dim", f"Napravljen {fmt_ts(m.get('created', ''))} na „{m.get('hostname', '?')}”, "
                    f"Claude Code {m.get('claude_code_version') or '?'}.")]
    lines = [("err", f"✖ Ispravno: {num(vr.ok)} od {num(vr.total)}.")]
    for a in (vr.bad[:6]):
        lines.append(("err", "  oštećen: " + a))
    for a in (vr.missing[:6]):
        lines.append(("err", "  nedostaje: " + a))
    if len(vr.bad) + len(vr.missing) > 12:
        lines.append(("err", f"  … i još {len(vr.bad) + len(vr.missing) - 12}"))
    return "✖ Backup nije ispravan", lines


def backup_report_lines(env: Env, res: BackupResult, ret: dict) -> List[Tuple[str, str]]:
    L: List[Tuple[str, str]] = []
    vr = res.verify
    if vr.passed:
        L.append(("ok", f"✔ Sačuvano {n_files(len(res.entries))} ({human(sum(e['size'] for e in res.entries))}) — "
                        f"svaki je proveren i identičan originalu (SHA-256)."))
    else:
        L.append(("err", f"✖ Provera: ispravno {num(vr.ok)} od {num(vr.total)} {vr.error}"))
    L.append(("", f"Backup: {env.tilde(res.path)}  ({human(res.size)})"))
    if res.convs:
        what = " i ".join(x for x, k in (("Pregled.html", "html"), ("Markdown", "md")) if k in res.formats)
        tail = " — raspakuj ZIP (dupli klik) pa otvori Pregled.html" if res.path.suffix == ".zip" and "html" in res.formats else ""
        L.append(("", f"Čitljivo: {n_convs(res.convs)} u {what}{tail}"))
    L.append(("head", "Sačuvano"))
    for cid, (n, b) in res.per_cat.items():
        L.append(("ok", f"✔ {CAT_TITLES[cid]} — {n_files(n)} · {human(b)}"))
    if res.failed:
        L.append(("head", "Nije moglo da se pročita"))
        for p, why in res.failed[:6]:
            L.append(("err", f"✖ {p} — {why}"))
        if len(res.failed) > 6:
            L.append(("err", f"  … i još {len(res.failed) - 6} (spisak je u izvestaj.txt)"))
    if res.unstable or res.new_after or res.changed_after:
        L.append(("head", "Claude je radio tokom backupa"))
        if res.unstable:
            L.append(("warn", f"⚠ {n_files(len(res.unstable))} se menjao dok je čitan — sačuvana je poslednja verzija."))
        if res.new_after or res.changed_after:
            L.append(("warn", f"⚠ Posle kopiranja: {num(len(res.new_after))} novih i {num(len(res.changed_after))} "
                              "promenjenih fajlova. Zatvori Claude i napravi backup ponovo da bi i to ušlo."))
    if res.problems:
        L.append(("warn", f"⚠ {plural(len(res.problems), 'upozorenje', 'upozorenja', 'upozorenja')} pri skeniranju (spisak je u izvestaj.txt)."))
    perm = [x for x in list(res.failed) + list(res.problems) if "not permitted" in x[1].lower()]
    if perm:
        L.append(("warn", f"⚠ macOS nije dozvolio čitanje {n_files(len(perm))} (najčešće projekti u Documents/Desktop). "
                          "Rešenje: System Settings → Privacy & Security → Full Disk Access → dodaj "
                          f"{sys.executable or 'python3'} (ili Terminal, ako backup pokrećeš ručno)."))
    L.append(("dim", "Namerno nije sačuvano: debug logovi, keš, login token i kod projekata — detalji u izvestaj.txt."))
    if ret.get("effective", 30) < RECOMMENDED_DAYS:
        L.append(("warn", f"⚠ Claude Code i dalje briše razgovore starije od {ret['effective']} dana — "
                          "izaberi „Podesi čuvanje istorije”."))
    return L


# ════════════════════════════════════════════════════════════ komandna linija

def cli_progress(enabled: bool):
    last = [0.0]

    def cb(phase, frac, a, b=""):
        if not enabled:
            return
        now = time.time()
        if now - last[0] < 0.1 and frac < 1:
            return
        last[0] = now
        names = {"copy": "Kopiram", "read": "Čitam razgovore", "render": "Pravim pregled", "verify": "Proveravam",
                 "rescan": "Završna provera", "plan": "Poredim", "restore": "Vraćam"}
        bar = "█" * int(24 * frac) + "░" * (24 - int(24 * frac))
        sys.stderr.write(f"\r\x1b[K{names.get(phase, phase):<16} {bar} {int(frac * 100):3d}%  {a}")
        sys.stderr.flush()
    return cb


def color(st: str, t: str) -> str:
    return (ST.get(st, "") + t + RESET) if sys.stdout.isatty() and st not in ("", "head") else t


def print_lines(lines: List[Tuple[str, str]]):
    for st, t in lines:
        if st == "head":
            print("\n" + color("accb", t))
        else:
            print(color(st, t))


def cmd_list(env: Env):
    inv = scan(env)
    ret = retention_info(env, inv.cc_version)
    print(color("accb", "Claude podaci na ovom računaru"))
    for cid, c in inv.cats.items():
        print(f"  {'✔' if c.files else '–'} {c.title:<40} {cat_note(inv, cid)}   [{cid}]")
    print(color("accb", "\nProjekti"))
    for p in sorted(inv.projects.values(), key=lambda p: -p.last):
        print(f"  {env.tilde(p.cwd) if p.cwd else p.key}  —  {n_convs(p.sessions)}, {human(p.size)}   [{p.key}]")
    print(color("accb", "\nČuvanje istorije"))
    print(f"  cleanupPeriodDays: {ret['cleanup'] if ret['cleanup'] is not None else 'nije podešeno (30)'}"
          f"   Claude Code: {inv.cc_version or '?'}")
    if inv.running:
        print(color("warn", f"\n⚠ Upaljeno: {', '.join(inv.running)}"))


def main(argv=None) -> int:
    try:
        sys.stdout.reconfigure(errors="replace")
        sys.stderr.reconfigure(errors="replace")
    except (AttributeError, ValueError):
        pass
    ap = argparse.ArgumentParser(
        prog="claude_backup.py",
        description="Bezbedan backup Claude Code razgovora, konteksta i podešavanja. Bez opcija otvara meni.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="Primeri:\n  python3 claude_backup.py\n  python3 claude_backup.py --backup --dest ~/Desktop\n"
               "  python3 claude_backup.py --verify ~/Desktop/ClaudeBackup_2026-10-16_210512.zip\n"
               "  python3 claude_backup.py --set-retention 3650\n"
               "  cd ~/Projects/moj-projekat && python3 ~/Downloads/claude_backup.py --project-backup\n"
               "  python3 claude_backup.py --schedule daily --at 03:00 --keep 14 --dest ~/ClaudeBackups\n"
               f"\nKategorije: {', '.join(CAT_TITLES)}")
    ap.add_argument("--backup", action="store_true", help="napravi backup bez menija")
    ap.add_argument("--only", help="samo ove kategorije, odvojene zarezom")
    ap.add_argument("--all", action="store_true", help="sve pronađene kategorije (i one isključene po difoltu)")
    ap.add_argument("--project", action="append", help="samo ovaj projekat (ključ iz --list); može više puta")
    ap.add_argument("--format", default="html,md", help="čitljivi formati: html,md ili none (original je uvek)")
    ap.add_argument("--dest", default="~/Desktop", help="folder za backup (default: Desktop)")
    ap.add_argument("--folder", action="store_true", help="običan folder umesto ZIP-a")
    ap.add_argument("--verify", metavar="PUTANJA", help="proveri postojeći backup")
    ap.add_argument("--restore", metavar="PUTANJA", help="vrati fajlove koji fale (bez --yes samo prikazuje plan)")
    ap.add_argument("--yes", action="store_true", help="potvrdi vraćanje")
    ap.add_argument("--set-retention", type=int, metavar="DANA", help="podesi cleanupPeriodDays (npr. 3650)")
    ap.add_argument("--keep", type=int, default=0, metavar="N",
                    help="uz automatski backup: posle uspešnog backupa zadrži samo N najnovijih AUTOMATSKIH backupa "
                         "u --dest folderu (ručne ne dira)")
    ap.add_argument("--notify", action="store_true", help="macOS notifikacija kad backup završi")
    ap.add_argument("--scheduled", action="store_true", help=argparse.SUPPRESS)
    ap.add_argument("--if-due", action="store_true", help=argparse.SUPPRESS)
    ap.add_argument("--schedule", choices=["daily", "weekly", "monthly"], help="uključi automatski backup (macOS)")
    ap.add_argument("--at", default="03:00", metavar="SS:MM", help="vreme za --schedule (default 03:00)")
    ap.add_argument("--unschedule", action="store_true", help="isključi automatski backup")
    ap.add_argument("--project-backup", nargs="?", const=".", metavar="FOLDER",
                    help=f"sačuvaj istoriju jednog projekta u njegov folder ({PROJECT_BACKUP_DIR}/); "
                         "bez FOLDER-a koristi trenutni folder")
    ap.add_argument("--zip", action="store_true", help="uz --project-backup: ZIP umesto običnog foldera")
    ap.add_argument("--list", action="store_true", help="prikaži šta je pronađeno")
    ap.add_argument("--version", action="version", version=f"claude_backup.py {TOOL_VERSION}")
    a = ap.parse_args(argv)
    env = Env()
    tty = sys.stderr.isatty()

    if a.list:
        cmd_list(env)
        st = schedule_status(env) if IS_MAC else None
        if st and not st.get("broken"):
            print(color("accb", "\nAutomatski backup"))
            print("  " + describe_schedule(env, st) + ("" if st["loaded"] else "  (nije učitan)"))
        return 0
    if a.unschedule:
        ok, msg = remove_schedule(env)
        print(color("ok" if ok else "err", ("✔ " if ok else "✖ ") + msg))
        return 0 if ok else 1
    if a.schedule:
        m = re.fullmatch(r"(\d{1,2})[:.](\d{2})", a.at.strip())
        if not m or int(m.group(1)) > 23 or int(m.group(2)) > 59:
            print(color("err", "✖ --at mora biti u obliku SS:MM, npr. 03:00"))
            return 1
        dest = Path(os.path.expanduser(a.dest if a.dest != "~/Desktop" else "~/ClaudeBackups"))
        ok, msg = install_schedule(env, a.schedule, int(m.group(1)), int(m.group(2)), dest, max(0, a.keep))
        if ok:
            print(color("ok", "✔ " + describe_schedule(env, schedule_status(env))))
        else:
            print(color("err", "✖ " + msg))
        return 0 if ok else 1
    if a.set_retention is not None:
        ok, msg, bak, ch = set_retention(env, a.set_retention)
        print(color("ok" if ok else "err", ("✔ " if ok else "✖ ") + msg))
        for c in ch:
            print("  " + c)
        if bak:
            print(f"  stari fajl: {env.tilde(bak)}")
        if not (a.backup or a.verify or a.restore):
            return 0 if ok else 1
    if a.verify:
        p = Path(os.path.expanduser(a.verify))
        vr = verify_backup(p, cli_progress(tty))
        if tty:
            sys.stderr.write("\r\x1b[K")
        t, lines = verify_message(env, p, vr)
        print(color("accb", t))
        print_lines(lines)
        return 0 if vr.passed else 2
    if a.restore:
        p = Path(os.path.expanduser(a.restore))
        vr = verify_backup(p, cli_progress(tty))
        if tty:
            sys.stderr.write("\r\x1b[K")
        if not vr.passed:
            print_lines(verify_message(env, p, vr)[1])
            print(color("err", "Vraćanje otkazano."))
            return 2
        items = plan_restore(env, vr.manifest, cli_progress(tty))
        if tty:
            sys.stderr.write("\r\x1b[K")
        only = set(a.only.split(",")) if a.only else None
        if only:
            items = [i for i in items if i.e.get("cat") in only]
        cnt: Dict[str, int] = {}
        for i in items:
            cnt[i.status] = cnt.get(i.status, 0) + 1
        print(f"Za vraćanje: {cnt.get('add', 0)} · isto: {cnt.get('same', 0)} · drugačije (ostaje tvoje): "
              f"{cnt.get('diff', 0)} · bez projekta: {cnt.get('noproj', 0)} · odbijeno: {cnt.get('unsafe', 0)}")
        if not a.yes:
            print("Ništa nije promenjeno. Dodaj --yes da vratiš fajlove koji fale.")
            return 0
        running = detect_running()
        if running:
            print(color("warn", f"⚠ Upaljeno: {', '.join(running)} — preporuka je da zatvoriš Claude."))
        rd = Reader(p)
        try:
            added, failed = apply_restore(rd, items, cli_progress(tty))
        finally:
            rd.close()
        if tty:
            sys.stderr.write("\r\x1b[K")
        print(color("ok", f"✔ Vraćeno: {n_files(added)}"))
        for s, w in failed:
            print(color("err", f"✖ {s} — {w}"))
        ret = retention_info(env, "")
        if ret["effective"] < RECOMMENDED_DAYS:
            print(color("warn", f"⚠ Claude Code briše razgovore starije od {ret['effective']} dana. Pre nego što ga otvoriš: "
                                f"python3 claude_backup.py --set-retention {RECOMMENDED_DAYS}"))
        return 0 if not failed else 1
    if a.project_backup:
        inv = scan(env)
        target = Path(os.path.expanduser(a.project_backup)).resolve()
        pr = project_for_path(inv, target)
        if not pr:
            print(color("err", f"✖ {env.tilde(target)} nije Claude Code projekat (nema razgovora za taj folder)."))
            known = sorted((p for p in inv.projects.values() if p.sessions and p.cwd), key=lambda p: -p.last)[:12]
            if known:
                print("Projekti sa razgovorima:")
                for p in known:
                    print(f"  {env.tilde(p.cwd)}  ({n_convs(p.sessions)})")
            return 1
        dest = Path(pr.cwd) / PROJECT_BACKUP_DIR
        err = prepare_project_dest(dest)
        if err:
            print(color("err", f"✖ Ne mogu da pišem u {env.tilde(dest)}: {err}"))
            return 1
        formats = set() if a.format.strip() == "none" else {f.strip() for f in a.format.split(",") if f.strip() in ("html", "md")}
        prompts = project_prompt_history(env, pr.cwd)
        res = run_backup(inv, [], {pr.key}, formats, dest, a.zip, cli_progress(tty),
                         selector=lambda i: project_entries(i, pr.key),
                         prefix=f"ClaudeBackup_{slug(Path(pr.cwd).name, 30)}",
                         scope={"type": "project", "cwd": pr.cwd, "key": pr.key},
                         extra=(lambda gen: gen("kucane-poruke.jsonl", prompts)) if prompts else None)
        if tty:
            sys.stderr.write("\r\x1b[K")
        print(color("accb", f"Istorija projekta {env.tilde(pr.cwd)}"))
        print_lines(backup_report_lines(env, res, retention_info(env, inv.cc_version)))
        return 0 if res.verify.passed else 2
    if a.backup:
        if a.scheduled and a.if_due:
            flag = force_flag_path(env)
            if flag.is_file():
                os.remove(flag)   # naš znak za „pokreni sada”
            else:
                due, why = schedule_due(env)
                if not due:
                    print(f"[{datetime.now():%Y-%m-%d %H:%M:%S}] {why}")
                    return 0
                print(f"[{datetime.now():%Y-%m-%d %H:%M:%S}] {why}")
        inv = scan(env)
        if a.only:
            cats = [c.strip() for c in a.only.split(",") if c.strip()]
            bad = [c for c in cats if c not in inv.cats]
            if bad:
                print(color("err", f"Nepoznate kategorije: {', '.join(bad)}"))
                return 1
        else:
            cats = [cid for cid, c in inv.cats.items() if c.files and (c.default or a.all)]
        projects = set(a.project) if a.project else None
        formats = set() if a.format.strip() == "none" else {f.strip() for f in a.format.split(",") if f.strip() in ("html", "md")}
        dest = Path(os.path.expanduser(a.dest))
        if a.scheduled:
            print(f"\n[{datetime.now():%Y-%m-%d %H:%M:%S}] Automatski backup → {dest}")
            if not dest.is_dir() and dest.parent.is_dir() and not str(dest).startswith("/Volumes/"):
                dest.mkdir(parents=True, exist_ok=True)
        app = App(None, env)  # samo za proveru lokacije
        err = app.dest_ok(dest)
        if err and str(dest).startswith("/Volumes/") and len(dest.parts) > 2 and not Path(*dest.parts[:3]).exists():
            err = f"Spoljni disk „{dest.parts[2]}” nije prikačen."
        if err:
            print(color("err", f"✖ {err} ({dest})"))
            if a.notify:
                notify("Claude Backup nije napravljen", f"{err} {env.tilde(dest)}")
            if a.scheduled:
                write_last_run(env, False, f"nije napravljen: {err} ({env.tilde(dest)})")
            return 1
        if inv.running:
            print(color("warn", f"⚠ Upaljeno: {', '.join(inv.running)} — za najtačniji backup zatvori Claude."))
        try:
            res = run_backup(inv, cats, projects, formats, dest, not a.folder, cli_progress(tty and not a.scheduled),
                             scope={"type": "full", "auto": True} if a.scheduled else None)
        except Exception as e:
            msg = os_err(e) if isinstance(e, OSError) else str(e)
            print(color("err", f"✖ Backup nije uspeo: {msg}"))
            if a.notify:
                notify("Claude Backup nije uspeo", msg[:200])
            if a.scheduled:
                write_last_run(env, False, f"nije uspeo: {msg[:200]}")
            raise
        if tty:
            sys.stderr.write("\r\x1b[K")
        print_lines(backup_report_lines(env, res, retention_info(env, inv.cc_version)))
        removed: List[Path] = []
        if a.keep and res.verify.passed and not a.folder:
            removed = rotate_backups(dest, a.keep, res.path)
            for r in removed:
                print(color("dim", f"  obrisan stari backup: {r.name}"))
        n = sum(1 for e in res.entries if e["cat"] == "code_sessions" and e["arc"].count("/") == 4
                and e["arc"].endswith(".jsonl") and UUID_RE.match(e["arc"].rsplit("/", 1)[1][:-6]))
        warn = []
        if res.failed:
            warn.append(f"{n_files(len(res.failed))} preskočeno")
        if res.problems:
            warn.append(plural(len(res.problems), "upozorenje", "upozorenja", "upozorenja") + " (vidi izvestaj.txt)")
        if res.verify.passed:
            summary = f"{n_convs(n)} · {human(res.size)} · proveren" + (" · " + ", ".join(warn) if warn else "")
            if removed:
                summary += f" · obrisano starih: {len(removed)}"
        else:
            summary = f"provera nije prošla: ispravno {res.verify.ok} od {res.verify.total} ({res.path.name})"
        if a.notify:
            notify("Claude Backup ✔" if res.verify.passed and not warn else
                   "Claude Backup ⚠" if res.verify.passed else "Claude Backup ✖", summary)
        if a.scheduled:
            write_last_run(env, res.verify.passed, summary, res.path)
        return 0 if res.verify.passed else 2

    # interaktivni meni
    if termios is None or not sys.stdin.isatty() or not sys.stdout.isatty():
        print("Meni radi samo u Terminalu. Za rad bez menija pogledaj: python3 claude_backup.py --help")
        return 1
    ui = UI()
    app = App(ui, env)
    ui.start()
    try:
        app.run()
    except KeyboardInterrupt:
        ui.stop()
        print("Prekinuto. U Claude-u ništa nije promenjeno.")
        return 130
    finally:
        ui.stop()
    if app.exit_lines:
        print(color("accb", "\nClaude Backup — poslednji rezultat"))
        print_lines(app.exit_lines)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except BrokenPipeError:      # npr. `… | head` — izlaz je prekinut, rad je već završen
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        sys.exit(0)
