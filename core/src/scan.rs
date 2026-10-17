//! Skeniranje: šta Claude ima na disku. Samo ČITA metapodatke — ništa ne menja.

use crate::env::{is_within, real, Env};
use crate::util::*;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const USER_FILES_AUTO_LIMIT: u64 = 500 * 1024 * 1024;
pub const PROJECT_BACKUP_DIR: &str = "claude-istorija";
pub const PER_SESSION_DIRS: [&str; 4] = ["file-history", "tasks", "todos", "session-env"];
pub const EXT_DIRS: [&str; 8] = ["skills", "agents", "commands", "plugins", "output-styles", "hooks", "rules", "workflows"];
pub const DESKTOP_CONFIG_ITEMS: [&str; 5] = [
    "claude_desktop_config.json",
    "config.json",
    "Claude Extensions",
    "Claude Extensions Settings",
    "extensions-installations.json",
];
pub const PROJECT_ITEMS: [&str; 4] = ["CLAUDE.md", "CLAUDE.local.md", ".mcp.json", ".claude"];
pub const PROJECT_SCOPED: [&str; 3] = ["code_sessions", "tmp_sessions", "project_files"];

pub fn skip_claude_top(name: &str) -> Option<&'static str> {
    Some(match name {
        "debug" => "debug logovi",
        "statsig" | "telemetry" => "analitika programa",
        "shell-snapshots" => "privremeni snimci terminala",
        "local" => "instalacija samog Claude Code programa",
        "ide" => "privremene veze sa editorom",
        "downloads" => "preuzimanja ažuriranja programa",
        "sessions" => "oznake trenutno upaljenih sesija",
        "cache" => "keš",
        ".credentials.json" => "login token — iz bezbednosnih razloga; posle vraćanja se samo ponovo uloguješ",
        _ => return None,
    })
}

pub struct CatDef {
    pub id: &'static str,
    pub title: &'static str,
    pub default: bool,
    pub desc: &'static str,
}

pub const CAT_DEFS: [CatDef; 9] = [
    CatDef { id: "code_sessions", title: "Claude Code razgovori", default: true,
        desc: "Svi razgovori iz terminala i Code taba, po projektima — sa pod-agentima, rezultatima alata i Claude-ovom memorijom projekta. Originalni .jsonl format koji Claude čita (~/.claude/projects)." },
    CatDef { id: "tmp_sessions", title: "Slike i scratchpad iz razgovora", default: true,
        desc: "Slike koje si nalepio u razgovore i Claude-ovi radni fajlovi (scratchpad). Stoje u privremenom folderu sistema koji se briše pri restartu ili čišćenju diska — sačuvaj ih dok postoje." },
    CatDef { id: "global", title: "Podešavanja, kontekst i istorija", default: true,
        desc: "Globalni CLAUDE.md, settings.json, ~/.claude.json (projekti, MCP serveri), istorija kucanih poruka (history.jsonl), planovi, zadaci i checkpoint snimci za rewind." },
    CatDef { id: "extensions", title: "Skillovi, agenti, komande, pluginovi", default: true,
        desc: "~/.claude/skills, agents, commands, plugins, output-styles, hooks i rules." },
    CatDef { id: "project_files", title: "Claude fajlovi u projektima", default: true,
        desc: "Iz foldera svakog projekta samo CLAUDE.md, CLAUDE.local.md, .mcp.json i .claude/ — bez koda." },
    CatDef { id: "desktop_code", title: "Code tab — lista razgovora", default: true,
        desc: "Kartice koje Claude aplikacija prikazuje u Code tabu (naslovi, arhivirano). Sami razgovori su u stavci „Claude Code razgovori”." },
    CatDef { id: "cowork", title: "Cowork sesije", default: true,
        desc: "Cowork razgovori i fajlovi koje je Claude napravio ili koje si ubacio u Cowork." },
    CatDef { id: "desktop_config", title: "Podešavanja Claude aplikacije", default: true,
        desc: "claude_desktop_config.json (MCP serveri), config.json i instalirane ekstenzije." },
    CatDef { id: "user_files", title: "Folderi ~/Claude i ~/Documents/Claude", default: false,
        desc: "Fajlovi koje Claude aplikacija čuva za tebe (artefakti, rezultati zakazanih zadataka). Uključeno je ako je manje od 500 MB; ako je veće, proveri da tu nemaš nešto svoje (npr. kod)." },
];

pub fn cat_title(id: &str) -> &'static str {
    CAT_DEFS.iter().find(|c| c.id == id).map(|c| c.title).unwrap_or("Ostalo")
}

pub fn uuid_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap())
}

#[derive(Clone, Debug)]
pub struct FEntry {
    pub src: PathBuf,
    pub arc: String,
    pub cat: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub mode: u32,
    pub proj: String,
    pub link: String,
}

impl FEntry {
    pub fn name(&self) -> String {
        self.src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    }
    pub fn is_session_file(&self) -> bool {
        let n = self.name();
        self.cat == "code_sessions"
            && self.arc.matches('/').count() == 4
            && n.ends_with(".jsonl")
            && uuid_re().is_match(&n[..n.len() - 6])
    }
}

#[derive(Clone, Debug)]
pub struct Cat {
    pub id: String,
    pub title: String,
    pub default: bool,
    pub desc: String,
    pub files: Vec<FEntry>,
}

impl Cat {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Proj {
    pub key: String,
    pub cwd: String,
    pub sessions: u64,
    pub size: u64,
    pub last: f64,
    pub slug: String,
}

#[derive(Clone, Debug)]
pub struct Inventory {
    pub env: Env,
    pub cats: Vec<Cat>,
    pub projects: BTreeMap<String, Proj>,
    pub excluded: Vec<(String, String)>,
    pub problems: Vec<(String, String)>,
    pub cc_version: String,
    pub running: Vec<String>,
}

impl Inventory {
    pub fn cat(&self, id: &str) -> &Cat {
        self.cats.iter().find(|c| c.id == id).expect("kategorija")
    }
    fn cat_mut(&mut self, id: &str) -> &mut Cat {
        self.cats.iter_mut().find(|c| c.id == id).expect("kategorija")
    }
}

pub struct Walk<'a> {
    pub env: &'a Env,
    pub problems: &'a mut Vec<(String, String)>,
    pub excluded: &'a mut Vec<(String, String)>,
}

pub struct WalkOpts<'b> {
    pub skip_top: Option<fn(&str) -> Option<&'static str>>,
    pub ignore_top: &'b [&'b str],
    pub skip_rel: Option<&'b dyn Fn(&str) -> Option<&'static str>>,
    pub proj_from_top: bool,
    pub proj: String,
}

impl<'b> Default for WalkOpts<'b> {
    fn default() -> Self {
        WalkOpts { skip_top: None, ignore_top: &[], skip_rel: None, proj_from_top: false, proj: String::new() }
    }
}

fn mtime_ns(m: &fs::Metadata) -> i64 {
    meta_mtime_ns(m)
}

impl<'a> Walk<'a> {
    /// Prolazi kroz folder i samo ČITA metapodatke. Prati simboličke linkove (bez petlji),
    /// preskače specijalne fajlove (socket/pipe) i beleži sve što nije moglo da se pročita.
    pub fn walk(&mut self, root: &Path, arc_root: &str, cat: &str, o: &WalkOpts) -> Vec<FEntry> {
        let mut out = vec![];
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let md = match fs::metadata(root) {
            Ok(m) => m,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.problems.push((self.env.token(root), os_err(&e)));
                }
                return out;
            }
        };
        if md.is_dir() {
            self.rec(root, "", arc_root, cat, o, &mut seen, &mut out);
        } else if md.is_file() {
            let link = fs::read_link(root).map(|l| l.to_string_lossy().to_string()).unwrap_or_default();
            out.push(FEntry {
                src: root.to_path_buf(),
                arc: arc_root.to_string(),
                cat: cat.into(),
                size: md.len(),
                mtime_ns: mtime_ns(&md),
                mode: meta_mode(&md),
                proj: o.proj.clone(),
                link,
            });
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn rec(&mut self, d: &Path, rel: &str, arc_root: &str, cat: &str, o: &WalkOpts, seen: &mut HashSet<PathBuf>, out: &mut Vec<FEntry>) {
        let d_real = real(d).unwrap_or_else(|_| d.to_path_buf());
        if !seen.insert(d_real) {
            return;
        }
        let rd = match fs::read_dir(d) {
            Ok(r) => r,
            Err(e) => {
                self.problems.push((self.env.token(d), format!("folder ne može da se otvori ({})", os_err(&e))));
                return;
            }
        };
        let mut names: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        for n in names {
            let p = d.join(&n);
            let r = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            if rel.is_empty() && o.ignore_top.contains(&n.as_str()) {
                continue;
            }
            if rel.is_empty() {
                if let Some(f) = o.skip_top {
                    if let Some(why) = f(&n) {
                        self.excluded.push((self.env.token(&p), why.into()));
                        continue;
                    }
                }
            }
            if let Some(f) = o.skip_rel {
                if let Some(why) = f(&r) {
                    self.excluded.push((self.env.token(&p), why.into()));
                    continue;
                }
            }
            let is_link = fs::symlink_metadata(&p).map(|m| m.file_type().is_symlink()).unwrap_or(false);
            let md = match fs::metadata(&p) {
                Ok(m) => m,
                Err(e) => {
                    let why = if is_link && e.kind() != std::io::ErrorKind::PermissionDenied {
                        "pokvarena prečica (link vodi na nešto što više ne postoji)".to_string()
                    } else {
                        format!("ne može da se pročita ({})", os_err(&e))
                    };
                    self.problems.push((self.env.token(&p), why));
                    continue;
                }
            };
            let link = if is_link { fs::read_link(&p).map(|l| l.to_string_lossy().to_string()).unwrap_or_default() } else { String::new() };
            if md.is_dir() {
                self.rec(&p, &r, arc_root, cat, o, seen, out);
            } else if md.is_file() {
                let pj = if !o.proj.is_empty() {
                    o.proj.clone()
                } else if o.proj_from_top && r.contains('/') {
                    r.split('/').next().unwrap_or("").to_string()
                } else {
                    String::new()
                };
                out.push(FEntry {
                    src: p.clone(),
                    arc: format!("{arc_root}/{r}"),
                    cat: cat.into(),
                    size: md.len(),
                    mtime_ns: mtime_ns(&md),
                    mode: meta_mode(&md),
                    proj: pj,
                    link,
                });
            } else {
                self.excluded.push((self.env.token(&p), "specijalan sistemski fajl (socket/pipe) — nije podatak".into()));
            }
        }
    }
}

pub fn first_cwd(p: &Path) -> String {
    let f = match fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    for line in BufReader::new(f).lines().take(81).map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(c) = v.get("cwd").and_then(|c| c.as_str()) {
                if !c.is_empty() {
                    return c.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn newest_version(files: &[FEntry]) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r#""version"\s*:\s*"(\d+\.\d+\.\d+)""#).unwrap());
    let mut jsonl: Vec<&FEntry> = files.iter().filter(|f| f.name().ends_with(".jsonl")).collect();
    jsonl.sort_by_key(|f| -f.mtime_ns);
    let mut best = String::new();
    for f in jsonl.iter().take(6) {
        let mut fh = match fs::File::open(&f.src) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let _ = fh.seek(SeekFrom::Start(f.size.saturating_sub(300_000)));
        let mut buf = vec![];
        if fh.read_to_end(&mut buf).is_err() {
            continue;
        }
        let tail = String::from_utf8_lossy(&buf);
        for c in re.captures_iter(&tail) {
            let v = c[1].to_string();
            if vtuple(&v) > vtuple(&best) {
                best = v;
            }
        }
    }
    best
}

pub fn detect_running() -> Vec<String> {
    let mut found = vec![];
    if cfg!(windows) {
        // Claude.exe = aplikacija; claude.exe = Claude Code (Windows ne razlikuje velika/mala slova)
        if let Ok(o) = command("tasklist").args(["/FI", "IMAGENAME eq claude.exe", "/FO", "CSV", "/NH"]).output() {
            if String::from_utf8_lossy(&o.stdout).to_lowercase().contains("\"claude.exe\"") {
                found.push("Claude".to_string());
            }
        }
        return found;
    }
    let me = std::process::id().to_string();
    for (name, label) in [("Claude", "Claude aplikacija"), ("claude", "Claude Code u terminalu")] {
        if let Ok(o) = command("pgrep").args(["-x", name]).output() {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.split_whitespace().any(|p| p.chars().all(|c| c.is_ascii_digit()) && p != me) {
                found.push(label.to_string());
            }
        }
    }
    found
}

pub fn load_json(p: &Path) -> Option<serde_json::Value> {
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn scan(env: &Env) -> Inventory {
    let mut problems = vec![];
    let mut excluded = vec![];
    let mut cats: Vec<Cat> = CAT_DEFS
        .iter()
        .map(|d| Cat { id: d.id.into(), title: d.title.into(), default: d.default, desc: d.desc.into(), files: vec![] })
        .collect();
    let c = env.claude.clone();
    let set = |cats: &mut Vec<Cat>, id: &str, files: Vec<FEntry>| {
        cats.iter_mut().find(|x| x.id == id).unwrap().files = files;
    };
    {
        let mut w = Walk { env, problems: &mut problems, excluded: &mut excluded };
        let f = w.walk(&c.join("projects"), "original/dot-claude/projects", "code_sessions",
                       &WalkOpts { proj_from_top: true, ..Default::default() });
        set(&mut cats, "code_sessions", f);
        let tmp = w.walk(&env.tmp, "original/claude-tmp", "tmp_sessions", &WalkOpts { proj_from_top: true, ..Default::default() });
        set(&mut cats, "tmp_sessions", tmp.into_iter().filter(|f| f.arc.matches('/').count() >= 3).collect());

        let mut ignore: Vec<&str> = vec!["projects"];
        ignore.extend(EXT_DIRS.iter());
        let mut g = w.walk(&c, "original/dot-claude", "global",
                           &WalkOpts { skip_top: Some(skip_claude_top), ignore_top: &ignore, ..Default::default() });
        if env.claude_json.is_file() {
            g.extend(w.walk(&env.claude_json, "original/claude.json", "global", &WalkOpts::default()));
        }
        set(&mut cats, "global", g);

        let mut ext = vec![];
        for n in EXT_DIRS {
            ext.extend(w.walk(&c.join(n), &format!("original/dot-claude/{n}"), "extensions", &WalkOpts::default()));
        }
        set(&mut cats, "extensions", ext);

        let dc = w.walk(&env.app.join("claude-code-sessions"), "original/claude-app/claude-code-sessions", "desktop_code", &WalkOpts::default());
        set(&mut cats, "desktop_code", dc);
        let cw = w.walk(&env.app.join("local-agent-mode-sessions"), "original/claude-app/local-agent-mode-sessions", "cowork", &WalkOpts::default());
        set(&mut cats, "cowork", cw);
        let mut dcf = vec![];
        for n in DESKTOP_CONFIG_ITEMS {
            dcf.extend(w.walk(&env.app.join(n), &format!("original/claude-app/{n}"), "desktop_config", &WalkOpts::default()));
        }
        set(&mut cats, "desktop_config", dcf);
        if env.app.is_dir() {
            let known: HashSet<&str> = DESKTOP_CONFIG_ITEMS.iter().copied().chain(["claude-code-sessions", "local-agent-mode-sessions"]).collect();
            let others = fs::read_dir(&env.app)
                .map(|rd| rd.filter_map(|e| e.ok()).any(|e| !known.contains(e.file_name().to_string_lossy().as_ref())))
                .unwrap_or(false);
            if others {
                w.excluded.push((format!("{}/(ostalo)", env.token(&env.app)),
                                 "program, keš, logovi i virtuelna mašina aplikacije — nisu tvoji podaci".into()));
            }
        }
        let mut uf = vec![];
        for p in [env.home.join("Claude"), env.home.join("Documents").join("Claude")] {
            if p.is_dir() {
                let arc = format!("original/home/{}", slug(&env.tilde(&p), 80));
                uf.extend(w.walk(&p, &arc, "user_files", &WalkOpts::default()));
            }
        }
        set(&mut cats, "user_files", uf);
    }
    let ufs: u64 = cats.iter().find(|x| x.id == "user_files").unwrap().size();
    cats.iter_mut().find(|x| x.id == "user_files").unwrap().default = ufs > 0 && ufs <= USER_FILES_AUTO_LIMIT;

    // projekti
    let mut projects: BTreeMap<String, Proj> = BTreeMap::new();
    for f in &cats.iter().find(|x| x.id == "code_sessions").unwrap().files {
        if f.proj.is_empty() {
            continue;
        }
        let pr = projects.entry(f.proj.clone()).or_insert_with(|| Proj { key: f.proj.clone(), ..Default::default() });
        pr.size += f.size;
        pr.last = pr.last.max(f.mtime_ns as f64 / 1e9);
        if f.is_session_file() {
            pr.sessions += 1;
            if pr.cwd.is_empty() {
                pr.cwd = first_cwd(&f.src);
            }
        }
    }
    if let Some(cj) = load_json(&env.claude_json) {
        if let Some(obj) = cj.get("projects").and_then(|p| p.as_object()) {
            for cwd in obj.keys() {
                if Path::new(cwd).is_absolute() {
                    let k = proj_key(cwd);
                    let pr = projects.entry(k.clone()).or_insert_with(|| Proj { key: k, ..Default::default() });
                    if pr.cwd.is_empty() {
                        pr.cwd = cwd.clone();
                    }
                }
            }
        }
    }

    let mut pf = vec![];
    let mut used = HashSet::new();
    {
        let mut w = Walk { env, problems: &mut problems, excluded: &mut excluded };
        let home_real = real(&env.home).unwrap_or(env.home.clone());
        let claude_real = real(&c).unwrap_or(c.clone());
        for pr in projects.values_mut() {
            if pr.cwd.is_empty() {
                continue;
            }
            let root = PathBuf::from(&pr.cwd);
            match fs::metadata(&root) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => continue,
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        w.problems.push((env.token(&root), format!("folder projekta ne može da se otvori ({})", os_err(&e))));
                    }
                    continue;
                }
            }
            let base = slug(root.file_name().map(|s| s.to_string_lossy().to_string()).as_deref().unwrap_or("root"), 40);
            let mut s = format!("{}-{}", base, &sha1_hex(pr.cwd.as_bytes())[..6]);
            while used.contains(&s) {
                s.push('x');
            }
            used.insert(s.clone());
            pr.slug = s.clone();
            let arc_base = format!("original/projekti/{s}");
            let is_home = real(&root).unwrap_or(root.clone()) == home_real;
            let wt = |r: &str| -> Option<&'static str> {
                if r == "worktrees" || r.starts_with("worktrees/") { Some("kopije koda (git worktrees)") } else { None }
            };
            for n in PROJECT_ITEMS {
                let p = root.join(n);
                if n == ".claude" {
                    if is_home || real(&p).map(|x| x == claude_real).unwrap_or(false) {
                        continue;
                    }
                    pf.extend(w.walk(&p, &format!("{arc_base}/.claude"), "project_files",
                                     &WalkOpts { skip_rel: Some(&wt), proj: pr.key.clone(), ..Default::default() }));
                } else {
                    let exists = fs::symlink_metadata(&p).is_ok();
                    if exists {
                        pf.extend(w.walk(&p, &format!("{arc_base}/{n}"), "project_files",
                                         &WalkOpts { proj: pr.key.clone(), ..Default::default() }));
                    }
                }
            }
        }
    }
    let mut inv = Inventory {
        env: env.clone(),
        cats,
        projects,
        excluded,
        problems,
        cc_version: String::new(),
        running: detect_running(),
    };
    inv.cat_mut("project_files").files = pf;
    inv.cc_version = newest_version(&inv.cat("code_sessions").files);
    inv
}

/// SHA-1 (samo za kratke, stabilne oznake foldera projekata — isto kao Python verzija).
pub fn sha1_hex(data: &[u8]) -> String {
    // mala, samostalna SHA-1 implementacija da ne vučemo još jedan paket
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|x| format!("{:08x}", x)).collect()
}

pub fn select_entries(inv: &Inventory, cats: &[String], projects: Option<&HashSet<String>>) -> Vec<FEntry> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    for cid in cats {
        for f in &inv.cat(cid).files {
            if let Some(ps) = projects {
                if PROJECT_SCOPED.contains(&cid.as_str()) && !ps.contains(&f.proj) {
                    continue;
                }
            }
            if seen.insert(f.arc.clone()) {
                out.push(f.clone());
            }
        }
    }
    out
}

pub fn cat_note(inv: &Inventory, cid: &str, projects: Option<&HashSet<String>>) -> String {
    let c = inv.cat(cid);
    let files: Vec<&FEntry> = c
        .files
        .iter()
        .filter(|f| match projects {
            Some(ps) if PROJECT_SCOPED.contains(&cid) => ps.contains(&f.proj),
            _ => true,
        })
        .collect();
    if files.is_empty() {
        return "nije pronađeno".into();
    }
    let size = human(files.iter().map(|f| f.size).sum());
    if cid == "code_sessions" {
        let n = files.iter().filter(|f| f.is_session_file()).count() as u64;
        return format!("{} · {}", n_convs(n), size);
    }
    if cid == "extensions" {
        let mut parts = vec![];
        for (d, one, few, many) in [("skills", "skill", "skilla", "skillova"), ("agents", "agent", "agenta", "agenata"),
                                    ("commands", "komanda", "komande", "komandi")] {
            let k = fs::read_dir(inv.env.claude.join(d))
                .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| !e.file_name().to_string_lossy().starts_with('.')).count())
                .unwrap_or(0) as u64;
            if k > 0 {
                parts.push(plural(k, one, few, many));
            }
        }
        if !parts.is_empty() {
            parts.push(size);
            return parts.join(" · ");
        }
    }
    format!("{} · {}", n_files(files.len() as u64), size)
}

// ---------------------------------------------------------------- jedan projekat

/// Projekat kome pripada folder (ili njegov podfolder). Sam home folder se ne računa kao projekat.
pub fn project_for_path<'a>(inv: &'a Inventory, path: &Path) -> Option<&'a Proj> {
    let rp = real(path).ok()?;
    let home = real(&inv.env.home).unwrap_or(inv.env.home.clone());
    if rp == home {
        return None;
    }
    let mut best: Option<&Proj> = None;
    let mut best_len = 0usize;
    for p in inv.projects.values() {
        if p.cwd.is_empty() || p.sessions == 0 {
            continue;
        }
        let c = real(Path::new(&p.cwd)).unwrap_or(PathBuf::from(&p.cwd));
        if c == home || c.parent().is_none() {
            continue;
        }
        if is_within(&rp, &c) {
            let l = c.to_string_lossy().len();
            if best.is_none() || l > best_len {
                best = Some(p);
                best_len = l;
            }
        }
    }
    best
}

/// Sve što pripada jednom projektu (originalni format, pa može i da se vrati).
pub fn project_entries(inv: &Inventory, key: &str) -> Vec<FEntry> {
    let cwd = inv.projects.get(key).map(|p| p.cwd.clone()).unwrap_or_default();
    let mut out: Vec<FEntry> = inv.cat("code_sessions").files.iter().filter(|f| f.proj == key).cloned().collect();
    let sids: HashSet<String> = out.iter().filter(|f| f.is_session_file()).map(|f| {
        let n = f.name();
        n[..n.len() - 6].to_string()
    }).collect();
    out.extend(inv.cat("tmp_sessions").files.iter().filter(|f| f.proj == key).cloned());
    out.extend(inv.cat("project_files").files.iter().filter(|f| f.proj == key).cloned());
    if !sids.is_empty() {
        for f in &inv.cat("global").files {
            let parts: Vec<&str> = f.arc.split('/').collect();
            if parts.len() >= 4 && parts[1] == "dot-claude" && PER_SESSION_DIRS.contains(&parts[2]) {
                let nm = parts[3];
                if sids.contains(nm) || sids.iter().any(|s| nm.starts_with(s.as_str())) {
                    out.push(f.clone());
                }
            }
        }
    }
    for f in &inv.cat("desktop_code").files {
        if f.src.extension().map(|e| e == "json").unwrap_or(false) && f.size < 2_000_000 {
            if let Some(d) = load_json(&f.src) {
                let cli = d.get("cliSessionId").and_then(|x| x.as_str()).unwrap_or("");
                let dcwd = d.get("cwd").and_then(|x| x.as_str()).unwrap_or("");
                if sids.contains(cli) || (!cwd.is_empty() && dcwd == cwd) {
                    out.push(f.clone());
                }
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|f| seen.insert(f.arc.clone()));
    out
}

/// Redovi iz ~/.claude/history.jsonl (poruke koje si kucao) koji pripadaju ovom projektu.
pub fn project_prompt_history(env: &Env, cwd: &str) -> Vec<u8> {
    let f = match fs::File::open(env.claude.join("history.jsonl")) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let mut out = String::new();
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if v.get("project").and_then(|p| p.as_str()) == Some(cwd) {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    out.into_bytes()
}

/// Pravi <projekat>/claude-istorija/ sa .gitignore, da istorija (može da sadrži ključeve) ne ode u git.
pub fn prepare_project_dest(dest: &Path) -> Result<(), String> {
    match fs::create_dir(dest) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(os_err(&e)),
    }
    let gi = dest.join(".gitignore");
    if fs::symlink_metadata(&gi).is_err() {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&gi).map_err(|e| os_err(&e))?;
        f.write_all("# Claude istorija — namerno van git-a (može da sadrži lozinke i API ključeve)\n*\n".as_bytes())
            .map_err(|e| os_err(&e))?;
    }
    Ok(())
}
