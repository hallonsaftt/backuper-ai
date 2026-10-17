//! Backup motor. Izvorne fajlove samo ČITA; piše isključivo u novi ClaudeBackup_… fajl/folder u odredištu.
//! Format (MANIFEST.json, putanje u arhivi) je isti kao kod Python verzije — backupi su međusobno zamenljivi.

use crate::env::{is_within, real, Env};
use crate::readable::build_readable;
use crate::scan::*;
use crate::i18n::t;
use crate::tf;
use crate::util::*;
use crate::verify::{verify_backup, VerifyResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zip::write::{SimpleFileOptions, ZipWriter};

pub const TOOL_VERSION: &str = "app-1.0";
const ZIP64_LIMIT: u64 = (1 << 31) - 1;

// ---------------------------------------------------------------- odredišta

pub trait Sink {
    fn put(&mut self, arc: &str, r: &mut dyn Read, size: u64, mtime_ns: i64, mode: u32) -> io::Result<()>;
    fn put_bytes(&mut self, arc: &str, data: &[u8]) -> io::Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0);
        self.put(arc, &mut &data[..], data.len() as u64, now, 0o600)
    }
    fn close(self: Box<Self>) -> io::Result<PathBuf>;
    fn abort(self: Box<Self>);
}

fn zip_dt(mtime_ns: i64) -> zip::DateTime {
    use chrono::{Datelike, Local, TimeZone, Timelike};
    let secs = (mtime_ns / 1_000_000_000).max(315_620_000); // ZIP ne zna datume pre 1980.
    match Local.timestamp_opt(secs, 0).single() {
        Some(d) => zip::DateTime::from_date_and_time(d.year() as u16, d.month() as u8, d.day() as u8,
                                                     d.hour() as u8, d.minute() as u8, d.second() as u8)
            .unwrap_or_default(),
        None => zip::DateTime::default(),
    }
}

fn is_our_part(p: &Path) -> bool {
    let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    n.starts_with("ClaudeBackup_") && n.ends_with(".part")
}

pub struct ZipSink {
    final_path: PathBuf,
    part: PathBuf,
    root: String,
    zw: Option<ZipWriter<File>>,
}

impl ZipSink {
    pub fn new(final_path: PathBuf, root: &str) -> io::Result<Self> {
        let part = final_path.with_file_name(format!("{}.part", final_path.file_name().unwrap().to_string_lossy()));
        let f = OpenOptions::new().write(true).create_new(true).open(&part)?;
        Ok(ZipSink { final_path, part, root: root.into(), zw: Some(ZipWriter::new(f)) })
    }
}

impl Sink for ZipSink {
    fn put(&mut self, arc: &str, r: &mut dyn Read, size: u64, mtime_ns: i64, mode: u32) -> io::Result<()> {
        let zw = self.zw.as_mut().unwrap();
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(mode & 0o777)
            .last_modified_time(zip_dt(mtime_ns))
            .large_file(size > ZIP64_LIMIT);
        zw.start_file(format!("{}/{}", self.root, arc), opts).map_err(io::Error::other)?;
        io::copy(r, zw)?;
        Ok(())
    }
    fn close(mut self: Box<Self>) -> io::Result<PathBuf> {
        let f = self.zw.take().unwrap().finish().map_err(io::Error::other)?;
        f.sync_all()?;
        drop(f);
        set_path_mode(&self.part, 0o600)?;
        if fs::symlink_metadata(&self.final_path).is_ok() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, tf!("{} već postoji", "{} already exists", self.final_path.display())));
        }
        fs::rename(&self.part, &self.final_path)?;
        Ok(self.final_path.clone())
    }
    fn abort(mut self: Box<Self>) {
        drop(self.zw.take());
        if is_our_part(&self.part) {
            let _ = fs::remove_file(&self.part); // samo naš nedovršeni fajl
        }
    }
}

pub struct DirSink {
    final_path: PathBuf,
    part: PathBuf,
}

impl DirSink {
    pub fn new(final_path: PathBuf) -> io::Result<Self> {
        let part = final_path.with_file_name(format!("{}.part", final_path.file_name().unwrap().to_string_lossy()));
        fs::create_dir(&part)?;
        set_path_mode(&part, 0o700)?;
        Ok(DirSink { final_path, part })
    }
}

impl Sink for DirSink {
    fn put(&mut self, arc: &str, r: &mut dyn Read, _size: u64, mtime_ns: i64, mode: u32) -> io::Result<()> {
        let t = self.part.join(arc);
        if let Some(p) = t.parent() {
            fs::create_dir_all(p)?;
        }
        let mut f = OpenOptions::new().write(true).create_new(true).open(&t)?;
        io::copy(r, &mut f)?;
        set_mode(&f, mode & 0o777);
        let mt = UNIX_EPOCH + Duration::from_nanos(mtime_ns.max(0) as u64);
        let _ = f.set_times(fs::FileTimes::new().set_modified(mt).set_accessed(mt));
        Ok(())
    }
    fn close(self: Box<Self>) -> io::Result<PathBuf> {
        if fs::symlink_metadata(&self.final_path).is_ok() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, tf!("{} već postoji", "{} already exists", self.final_path.display())));
        }
        fs::rename(&self.part, &self.final_path)?;
        Ok(self.final_path.clone())
    }
    fn abort(self: Box<Self>) {
        if is_our_part(&self.part) {
            let _ = fs::remove_dir_all(&self.part); // samo naš nedovršeni folder
        }
    }
}

// ---------------------------------------------------------------- čitanje izvora

pub struct Captured {
    pub spool: tempfile::SpooledTempFile,
    pub sha: String,
    pub size: u64,
    pub stable: bool,
    pub mtime_ns: i64,
    pub mode: u32,
}

fn st_key(m: &fs::Metadata) -> (i64, u64) {
    (meta_mtime_ns(m), m.len())
}

/// Pročita fajl u privremenu memoriju i izračuna SHA-256. Ako se fajl menjao dok je čitan
/// (npr. Claude upravo piše), pokuša ponovo.
pub fn capture(src: &Path) -> io::Result<Captured> {
    use sha2::{Digest, Sha256};
    for attempt in 0..3 {
        let m1 = fs::metadata(src)?;
        if !m1.is_file() {
            return Err(io::Error::other(t("nije običan fajl", "not a regular file")));
        }
        let mut spool = tempfile::spooled_tempfile(64 << 20);
        let mut h = Sha256::new();
        let mut n = 0u64;
        let mut f = File::open(src)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let k = f.read(&mut buf)?;
            if k == 0 {
                break;
            }
            h.update(&buf[..k]);
            spool.write_all(&buf[..k])?;
            n += k as u64;
        }
        let m2 = fs::metadata(src)?;
        let stable = st_key(&m1) == st_key(&m2) && m2.len() == n;
        if stable || attempt == 2 {
            spool.seek(SeekFrom::Start(0))?;
            return Ok(Captured {
                spool,
                sha: hex::encode(h.finalize()),
                size: n,
                stable,
                mtime_ns: if stable { st_key(&m1).0 } else { st_key(&m2).0 },
                mode: meta_mode(&m2),
            });
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err(io::Error::other(t("neočekivano", "unexpected")))
}

pub fn unique_name(dest: &Path, prefix: &str) -> String {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let mut base = format!("{prefix}_{stamp}");
    let mut n = 1;
    while [".zip", ".zip.part", "", ".part"].iter().any(|ext| fs::symlink_metadata(dest.join(format!("{base}{ext}"))).is_ok()) {
        n += 1;
        base = format!("{prefix}_{stamp}_{n}");
    }
    base
}

/// Da li sme da se piše u `p`: postoji, može da se piše, i nije unutar Claude-ovih foldera.
pub fn dest_ok(env: &Env, p: &Path) -> Option<String> {
    let rp = real(p).unwrap_or(p.to_path_buf());
    match fs::metadata(&rp) {
        Ok(m) if m.is_dir() => {}
        _ => return Some(t("Taj folder ne postoji.", "That folder doesn't exist.").into()),
    }
    let probe = rp.join(format!(".claude-backup-probe-{}", std::process::id()));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe); // naš probni fajl
        }
        Err(e) => return Some(tf!("U taj folder ne može da se piše ({}).", "Can't write to that folder ({}).", os_err(&e))),
    }
    for src in [&env.claude, &env.app, &env.tmp] {
        let s = real(src).unwrap_or(src.clone());
        if is_within(&rp, &s) {
            return Some(t("Backup ne sme da ide u Claude-ov folder — izaberi drugi.", "A backup can't go inside Claude's own folder — pick another one.").into());
        }
    }
    None
}

// ---------------------------------------------------------------- opcije i rezultat

#[derive(Clone, Debug)]
pub struct BackupOpts {
    pub cats: Vec<String>,
    pub projects: Option<HashSet<String>>,
    pub html: bool,
    pub md: bool,
    pub dest: PathBuf,
    pub as_zip: bool,
    pub prefix: String,
    /// Some(ključ) = istorija jednog projekta
    pub project_key: Option<String>,
    pub scope: Value,
    pub extra: Vec<(String, Vec<u8>)>,
}

impl BackupOpts {
    pub fn full(cats: Vec<String>, dest: PathBuf) -> Self {
        BackupOpts {
            cats,
            projects: None,
            html: true,
            md: true,
            dest,
            as_zip: true,
            prefix: "ClaudeBackup".into(),
            project_key: None,
            scope: json!({"type": "full"}),
            extra: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CatCount {
    pub id: String,
    pub title: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackupResult {
    pub path: String,
    pub path_tilde: String,
    pub is_zip: bool,
    pub size: u64,
    pub files: u64,
    pub bytes: u64,
    pub convs: u64,
    pub has_html: bool,
    pub has_md: bool,
    /// name of the HTML overview inside this backup (Overview.html / Pregled.html)
    pub overview: String,
    pub verify: VerifyResult,
    pub failed: Vec<(String, String)>,
    pub unstable: Vec<String>,
    pub new_after: Vec<String>,
    pub changed_after: Vec<String>,
    pub per_cat: Vec<CatCount>,
    pub problems: Vec<(String, String)>,
    pub seconds: f64,
}

pub fn select_for(inv: &Inventory, o: &BackupOpts) -> Vec<FEntry> {
    match &o.project_key {
        Some(k) => project_entries(inv, k),
        None => select_entries(inv, &o.cats, o.projects.as_ref()),
    }
}

// ---------------------------------------------------------------- tekstovi u backupu

pub fn readme_text(o: &BackupOpts, created: &str, env: &Env) -> String {
    let nm = crate::i18n::names();
    let win = cfg!(windows);
    let mut l: Vec<String> = vec![format!("CLAUDE BACKUP — {created}"), "=".repeat(40), String::new()];
    let is_proj = o.scope.get("type").and_then(|t| t.as_str()) == Some("project");
    if is_proj {
        let cwd = o.scope.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
        l.push(tf!("Ovo je istorija JEDNOG projekta: {}", "This is the history of ONE project: {}", env.tilde(Path::new(cwd))));
        l.push(t("(razgovori, memorija projekta, slike, checkpoint snimci i Code tab kartice tog projekta)",
                 "(that project's conversations, project memory, images, checkpoints and Code tab cards)").into());
        l.push(String::new());
    }
    l.push(t("Šta je ovde", "What's in here").into());
    l.push(t("-----------", "--------------").into());
    if is_proj {
        l.push(format!("{:<21}{}", nm.prompts, t("Poruke koje si kucao u ovom projektu (iz ~/.claude/history.jsonl).",
                                                 "The prompts you typed in this project (from ~/.claude/history.jsonl).")));
    }
    if o.html {
        l.push(format!("{:<18}{}", nm.overview, t("Svi razgovori za čitanje u browseru (dupli klik).", "Every conversation, readable in a browser (double-click).")));
    }
    if o.md {
        l.push(format!("{:<18}{}", format!("{}/", nm.md_dir), t("Isti razgovori kao Markdown (.md) fajlovi; spisak je u INDEX.md.",
                                                               "The same conversations as Markdown (.md) files; the list is in INDEX.md.")));
    }
    let body: Vec<String> = if crate::i18n::sr() {
        vec![
            "original/         TAČNE kopije Claude fajlova, bajt po bajt. Iz ovoga se vraća:".into(),
            "  dot-claude/        = ~/.claude (razgovori, memorija, podešavanja, skillovi…)".into(),
            "  claude.json        = ~/.claude.json".into(),
            if win { "  claude-app/        = %APPDATA%\\Claude (Code tab lista, Cowork…)" } else { "  claude-app/        = ~/Library/Application Support/Claude (Code tab lista, Cowork…)" }.into(),
            if win { "  claude-tmp/        = %TEMP%\\claude (slike i scratchpad iz razgovora)" } else { "  claude-tmp/        = /private/tmp/claude-<broj> (slike i scratchpad iz razgovora)" }.into(),
            "  projekti/          = CLAUDE.md, .claude/ i .mcp.json iz foldera projekata".into(),
            format!("                       (tačna putanja svakog projekta je u {})", nm.report),
            "MANIFEST.json     Spisak svih fajlova: odakle su, veličina, vreme izmene, SHA-256.".into(),
            "SHA256SUMS.txt    Kontrolni zbirovi svih fajlova.".into(),
            format!("{:<18}Šta je sačuvano, šta nije i zašto.", nm.report),
            String::new(),
            if win { "Folder .claude je običan folder — vidi se u Exploreru." } else { "Folderi čije ime počinje tačkom (.claude) su u Finderu sakriveni — Cmd+Shift+. ih prikazuje." }.into(),
            String::new(),
            "Kako da proverim da je backup ispravan".into(),
            "--------------------------------------".into(),
            "Claude Backup aplikacija  →  „Backupi i vraćanje”  →  „Proveri”".into(),
            if win { "ili u Git Bash-u, u ovom folderu:   sha256sum -c SHA256SUMS.txt" } else { "ili u Terminalu, u ovom folderu:   shasum -a 256 -c SHA256SUMS.txt" }.into(),
            String::new(),
            "Kako da vratim".into(),
            "--------------".into(),
            "1. Zatvori Claude.".into(),
            "2. Claude Backup aplikacija  →  „Backupi i vraćanje”  →  „Vrati…” kod ovog backupa.".into(),
            "   Program samo DODAJE fajlove koji fale. Tvoje postojeće fajlove ne dira.".into(),
            "3. Na novom računaru stavi projekte na iste putanje kao ranije, da bi".into(),
            "   „claude --resume” u tom folderu pronašao stare razgovore.".into(),
            String::new(),
            "Privatnost".into(),
            "----------".into(),
            "Razgovori mogu da sadrže lozinke i API ključeve koje si nalepio ili koje je Claude".into(),
            "pročitao iz .env fajlova. Čuvaj ovaj backup kao privatan.".into(),
            "Login token (~/.claude/.credentials.json) namerno nije sačuvan — posle vraćanja se".into(),
            "samo ponovo uloguješ.".into(),
            String::new(),
            "Chat tab razgovori nisu deo ovog backupa: oni su na Anthropic serveru.".into(),
            "Izvoz: claude.ai → Settings → Privacy → Export data.".into(),
            String::new(),
            format!("Napravila Claude Backup aplikacija ({TOOL_VERSION})"),
        ]
    } else {
        vec![
            "original/         EXACT copies of Claude's files, byte for byte. Restores come from here:".into(),
            "  dot-claude/        = ~/.claude (conversations, memory, settings, skills…)".into(),
            "  claude.json        = ~/.claude.json".into(),
            if win { "  claude-app/        = %APPDATA%\\Claude (Code tab list, Cowork…)" } else { "  claude-app/        = ~/Library/Application Support/Claude (Code tab list, Cowork…)" }.into(),
            if win { "  claude-tmp/        = %TEMP%\\claude (images and scratchpad from conversations)" } else { "  claude-tmp/        = /private/tmp/claude-<number> (images and scratchpad from conversations)" }.into(),
            "  projekti/          = CLAUDE.md, .claude/ and .mcp.json from project folders".into(),
            format!("                       (each project's exact path is in {})", nm.report),
            "MANIFEST.json     Every file: where it came from, size, modification time, SHA-256.".into(),
            "SHA256SUMS.txt    Checksums of every file.".into(),
            format!("{:<18}What was saved, what wasn't, and why.", nm.report),
            String::new(),
            if win { "The .claude folder is a normal folder — Explorer shows it." } else { "Folders whose name starts with a dot (.claude) are hidden in Finder — Cmd+Shift+. shows them." }.into(),
            String::new(),
            "How to check the backup is intact".into(),
            "---------------------------------".into(),
            "Claude Backup app  →  “Backups & restore”  →  “Verify”".into(),
            if win { "or in Git Bash, in this folder:   sha256sum -c SHA256SUMS.txt" } else { "or in Terminal, in this folder:   shasum -a 256 -c SHA256SUMS.txt" }.into(),
            String::new(),
            "How to restore".into(),
            "--------------".into(),
            "1. Quit Claude.".into(),
            "2. Claude Backup app  →  “Backups & restore”  →  “Restore…” next to this backup.".into(),
            "   It only ADDS files that are missing. Your existing files are never touched.".into(),
            "3. On a new computer, put your projects at the same paths as before, so that".into(),
            "   “claude --resume” in that folder finds the old conversations.".into(),
            String::new(),
            "Privacy".into(),
            "-------".into(),
            "Conversations can contain passwords and API keys you pasted, or that Claude".into(),
            "read from .env files. Keep this backup private.".into(),
            "The login token (~/.claude/.credentials.json) is deliberately not saved — after".into(),
            "restoring you simply log in again.".into(),
            String::new(),
            "Chat tab conversations are not part of this backup: they live on Anthropic's servers.".into(),
            "Export: claude.ai → Settings → Privacy → Export data.".into(),
            String::new(),
            format!("Made by the Claude Backup app ({TOOL_VERSION})"),
        ]
    };
    l.extend(body);
    l.join("\n") + "\n"
}

#[allow(clippy::too_many_arguments)]
fn report_text(inv: &Inventory, files: &[Value], failed: &[(String, String)], unstable: &[String],
               per_cat: &[CatCount], o: &BackupOpts, created: &str, convs: u64) -> String {
    let env = &inv.env;
    let nm = crate::i18n::names();
    let host = hostname();
    let mut l: Vec<String> = vec![
        tf!("IZVEŠTAJ — Claude Backup {}", "REPORT — Claude Backup {}", created),
        "=".repeat(40),
        String::new(),
        tf!("Računar: {}   Korisnik: {}   Claude Code: {}", "Computer: {}   User: {}   Claude Code: {}", host,
            env.home.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            if inv.cc_version.is_empty() { t("nepoznato", "unknown") } else { &inv.cc_version }),
        String::new(),
        t("Sačuvano", "Saved").into(),
        t("--------", "-----").into(),
    ];
    for c in per_cat {
        l.push(format!("  ✔ {:<40} {:>14}   {:>10}", c.title, n_files(c.files), human(c.bytes)));
    }
    let total: u64 = files.iter().map(|f| f["size"].as_u64().unwrap_or(0)).sum();
    l.push(String::new());
    l.push(tf!("Ukupno: {}, {}. Svaki fajl je kopiran bajt po bajt i proveren SHA-256 zbirom.",
               "Total: {}, {}. Every file was copied byte for byte and checked with SHA-256.", n_files(files.len() as u64), human(total)));
    l.push(String::new());
    if convs > 0 {
        let mut fm = vec![];
        if o.html {
            fm.push(format!("HTML ({})", nm.overview));
        }
        if o.md {
            fm.push(format!("Markdown ({}/)", nm.md_dir));
        }
        l.push(tf!("Čitljiva verzija: {} — {}", "Readable version: {} — {}", n_convs(convs), fm.join(" + ")));
        l.push(String::new());
    }
    l.push(t("Projekti", "Projects").into());
    l.push("--------".into());
    let mut prs: Vec<&Proj> = inv.projects.values().collect();
    prs.sort_by(|a, b| b.last.partial_cmp(&a.last).unwrap_or(std::cmp::Ordering::Equal));
    for pr in prs {
        let sel = match (&o.project_key, &o.projects) {
            (Some(k), _) => *k == pr.key,
            (None, Some(ps)) => ps.contains(&pr.key),
            (None, None) => true,
        };
        l.push(format!("  {} {}", if sel { "✔" } else { "–" }, if pr.cwd.is_empty() { pr.key.clone() } else { env.tilde(Path::new(&pr.cwd)) }));
        l.push(tf!("      razgovori: ~/.claude/projects/{}   ({})", "      conversations: ~/.claude/projects/{}   ({})", pr.key, n_convs(pr.sessions)));
        if !pr.slug.is_empty() {
            l.push(tf!("      Claude fajlovi projekta u backupu: original/projekti/{}/", "      the project's Claude files in this backup: original/projekti/{}/", pr.slug));
        }
    }
    if !failed.is_empty() {
        l.push(String::new());
        l.push(t("NIJE MOGLO DA SE PROČITA (nije u backupu)", "COULD NOT BE READ (not in this backup)").into());
        l.push("-----------------------------------------".into());
        for (p, w) in failed {
            l.push(format!("  ✖ {p} — {w}"));
        }
    }
    if !unstable.is_empty() {
        l.push(String::new());
        l.push(t("Menjali su se dok je backup radio (Claude je bio upaljen) — sačuvana je poslednja verzija:",
                 "Changed while the backup was running (Claude was open) — the latest version was saved:").into());
        for p in unstable {
            l.push(format!("  ⚠ {p}"));
        }
    }
    if !inv.problems.is_empty() {
        l.push(String::new());
        l.push(t("Upozorenja pri skeniranju", "Warnings while scanning").into());
        l.push("-------------------------".into());
        for (p, w) in &inv.problems {
            l.push(format!("  ⚠ {p} — {w}"));
        }
    }
    l.push(String::new());
    l.push(t("Namerno nije sačuvano", "Deliberately not saved").into());
    l.push("---------------------".into());
    let mut seen = HashSet::new();
    for (p, w) in &inv.excluded {
        if seen.insert((p.clone(), w.clone())) {
            l.push(format!("  – {p} — {w}"));
        }
    }
    l.push(t("  – kod projekata — čuvaju se samo Claude fajlovi iz projekata", "  – project code — only Claude's files are taken from projects").into());
    l.push(t("  – Chat tab — nalazi se na Anthropic serveru (Settings → Privacy → Export data)",
             "  – Chat tab — it lives on Anthropic's servers (Settings → Privacy → Export data)").into());
    l.join("\n") + "\n"
}

pub use crate::util::hostname;

// ---------------------------------------------------------------- glavni posao

pub fn run_backup(inv: &Inventory, o: &BackupOpts, cb: Cb, cancel: Option<&AtomicBool>) -> io::Result<BackupResult> {
    let env = &inv.env;
    let t0 = Instant::now();
    let rep = Reporter::new(cb);
    let entries = select_for(inv, o);
    let cats: Vec<String> = if o.project_key.is_some() {
        let mut v: Vec<String> = vec![];
        for e in &entries {
            if !v.contains(&e.cat) {
                v.push(e.cat.clone());
            }
        }
        v
    } else {
        o.cats.clone()
    };
    fs::create_dir_all(&o.dest).ok();
    let name = unique_name(&o.dest, &o.prefix);
    let final_path = o.dest.join(if o.as_zip { format!("{name}.zip") } else { name.clone() });
    let mut sink: Box<dyn Sink> = if o.as_zip {
        Box::new(ZipSink::new(final_path, &name)?)
    } else {
        Box::new(DirSink::new(final_path)?)
    };
    let mut files_meta: Vec<Value> = vec![];
    let mut generated: Vec<Value> = vec![];
    let mut failed: Vec<(String, String)> = vec![];
    let mut unstable: Vec<String> = vec![];
    let mut per: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let total: u64 = entries.iter().map(|e| e.size).sum::<u64>().max(1);
    let created = chrono::Local::now().format(t("%d.%m.%Y. u %H:%M", "%b %-d, %Y at %H:%M")).to_string();

    let result: io::Result<(PathBuf, u64)> = (|| {
        let mut done = 0u64;
        for (i, e) in entries.iter().enumerate() {
            if cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, t("prekinuto", "cancelled")));
            }
            rep.emit("copy", done as f64 / total as f64,
                     tf!("{} / {} fajlova · {} / {}", "{} / {} files · {} / {}", num(i as u64 + 1), num(entries.len() as u64), human(done), human(total)),
                     env.tilde(&e.src), false);
            let mut cap = match capture(&e.src) {
                Ok(c) => c,
                Err(ex) => {
                    failed.push((env.token(&e.src), os_err(&ex)));
                    continue;
                }
            };
            sink.put(&e.arc, &mut cap.spool, cap.size, cap.mtime_ns, cap.mode)?;
            let mut rec = json!({
                "arc": e.arc, "src": env.token(&e.src), "cat": e.cat, "size": cap.size, "sha256": cap.sha,
                "mtime_ns": cap.mtime_ns, "mode": cap.mode & 0o7777,
            });
            if !e.proj.is_empty() {
                rec["proj"] = json!(e.proj);
            }
            if e.cat == "project_files" {
                if let Some(p) = inv.projects.get(&e.proj) {
                    rec["proj_root"] = json!(env.token(Path::new(&p.cwd)));
                }
            }
            if !e.link.is_empty() {
                rec["symlink"] = json!(e.link);
            }
            if !cap.stable {
                unstable.push(env.token(&e.src));
            }
            let c = per.entry(e.cat.clone()).or_insert((0, 0));
            c.0 += 1;
            c.1 += cap.size;
            files_meta.push(rec);
            done += e.size;
        }
        let mut gen = |arc: &str, data: Vec<u8>| -> io::Result<()> {
            sink.put_bytes(arc, &data)?;
            generated.push(json!({"arc": arc, "size": data.len(), "sha256": sha_bytes(&data)}));
            Ok(())
        };
        let mut convs = 0u64;
        if o.html || o.md {
            convs = build_readable(inv, &entries, o.html, o.md, &mut gen, &rep)? as u64;
        }
        for (arc, data) in &o.extra {
            gen(arc, data.clone())?;
        }
        let per_cat: Vec<CatCount> = cats.iter().filter_map(|cid| per.get(cid).map(|(n, b)| CatCount {
            id: cid.clone(), title: cat_title(cid).into(), files: *n, bytes: *b })).collect();
        let nm = crate::i18n::names();
        gen(nm.readme, readme_text(o, &created, env).into_bytes())?;
        gen(nm.report, report_text(inv, &files_meta, &failed, &unstable, &per_cat, o, &created, convs).into_bytes())?;
        drop(gen);
        let mut categories = serde_json::Map::new();
        for cid in &cats {
            let (n, b) = per.get(cid).copied().unwrap_or((0, 0));
            categories.insert(cid.clone(), json!({"title": cat_title(cid), "files": n, "bytes": b}));
        }
        let mut projects = serde_json::Map::new();
        for (k, p) in &inv.projects {
            let sel = match (&o.project_key, &o.projects) {
                (Some(pk), _) => pk == k,
                (None, Some(ps)) => ps.contains(k),
                (None, None) => true,
            };
            projects.insert(k.clone(), json!({"cwd": p.cwd, "selected": sel, "slug": p.slug}));
        }
        let mut excl = vec![];
        let mut seen = HashSet::new();
        for (p, w) in &inv.excluded {
            if seen.insert((p.clone(), w.clone())) {
                excl.push(json!({"path": p, "reason": w}));
            }
        }
        let formats: Vec<&str> = {
            let mut v = vec![];
            if o.html { v.push("html"); }
            if o.md { v.push("md"); }
            v.push("original");
            v.sort();
            v
        };
        let manifest = json!({
            "format": "claude-backup", "tool_version": TOOL_VERSION, "scope": o.scope,
            "created": now_iso_utc(), "hostname": hostname(),
            "home": env.home.to_string_lossy(), "tmp_root": env.tmp.to_string_lossy(),
            "claude_code_version": inv.cc_version,
            "categories": categories, "projects": projects, "formats": formats,
            "files": files_meta, "generated": generated,
            "failed": failed.iter().map(|(p, w)| json!({"src": p, "error": w})).collect::<Vec<_>>(),
            "changed_during_backup": unstable, "excluded": excl,
        });
        sink.put_bytes("MANIFEST.json", serde_json::to_string_pretty(&manifest).unwrap().as_bytes())?;
        let sums: String = files_meta.iter().chain(generated.iter())
            .map(|f| format!("{}  {}\n", f["sha256"].as_str().unwrap_or(""), f["arc"].as_str().unwrap_or("")))
            .collect();
        sink.put_bytes("SHA256SUMS.txt", sums.as_bytes())?;
        Ok((PathBuf::new(), convs))
    })();

    let (convs, out) = match result {
        Ok((_, convs)) => (convs, sink.close()?),
        Err(e) => {
            sink.abort();
            return Err(e);
        }
    };
    let vr = verify_backup(&out, cb);
    rep.emit("rescan", 1.0, t("Poredim backup sa trenutnim stanjem…", "Comparing the backup with the current state…"), "", true);
    let now_inv = scan(env);
    let now = select_for(&now_inv, o);
    let saved: HashMap<String, (u64, i64)> = files_meta.iter().map(|f| (
        f["src"].as_str().unwrap_or("").to_string(),
        (f["size"].as_u64().unwrap_or(0), f["mtime_ns"].as_i64().unwrap_or(0)),
    )).collect();
    let mut new_after = vec![];
    let mut changed_after = vec![];
    for e in &now {
        let t = env.token(&e.src);
        match saved.get(&t) {
            None => new_after.push(t),
            Some(&(s, m)) if s != e.size || m != e.mtime_ns => changed_after.push(t),
            _ => {}
        }
    }
    let size = if out.is_file() { fs::metadata(&out).map(|m| m.len()).unwrap_or(0) } else {
        files_meta.iter().chain(generated.iter()).map(|f| f["size"].as_u64().unwrap_or(0)).sum()
    };
    let per_cat: Vec<CatCount> = cats.iter().filter_map(|cid| per.get(cid).map(|(n, b)| CatCount {
        id: cid.clone(), title: cat_title(cid).into(), files: *n, bytes: *b })).collect();
    Ok(BackupResult {
        path: out.to_string_lossy().into(),
        path_tilde: env.tilde(&out),
        is_zip: o.as_zip,
        size,
        files: files_meta.len() as u64,
        bytes: files_meta.iter().map(|f| f["size"].as_u64().unwrap_or(0)).sum(),
        convs,
        has_html: o.html && convs > 0,
        has_md: o.md && convs > 0,
        overview: crate::i18n::names().overview.into(),
        verify: vr,
        failed,
        unstable,
        new_after,
        changed_after,
        per_cat,
        problems: inv.problems.clone(),
        seconds: t0.elapsed().as_secs_f64(),
    })
}

/// Tekstualni izveštaj (za terminal, log zakazanog backupa i notifikaciju).
pub fn report_lines(r: &BackupResult) -> Vec<(String, String)> {
    let mut l: Vec<(String, String)> = vec![];
    let vr = &r.verify;
    if vr.passed() {
        l.push(("ok".into(), tf!("✔ Sačuvano {} ({}) — svaki je proveren i identičan originalu (SHA-256).", "✔ Saved {} ({}) — each one checked and identical to the original (SHA-256).", n_files(r.files), human(r.bytes))));
    } else {
        l.push(("err".into(), tf!("✖ Provera: ispravno {} od {} {}", "✖ Verification: {} of {} intact {}", num(vr.ok), num(vr.total), vr.error)));
    }
    l.push((String::new(), format!("Backup: {}  ({})", r.path_tilde, human(r.size))));
    if r.convs > 0 {
        let mut what = vec![];
        if r.has_html { what.push(crate::i18n::names().overview); }
        if r.has_md { what.push("Markdown"); }
        l.push((String::new(), tf!("Čitljivo: {} u {}", "Readable: {} in {}", n_convs(r.convs), what.join(t(" i ", " and ")))));
    }
    l.push(("head".into(), t("Sačuvano", "Saved").into()));
    for c in &r.per_cat {
        l.push(("ok".into(), format!("✔ {} — {} · {}", c.title, n_files(c.files), human(c.bytes))));
    }
    if !r.failed.is_empty() {
        l.push(("head".into(), t("Nije moglo da se pročita", "Could not be read").into()));
        for (p, w) in r.failed.iter().take(6) {
            l.push(("err".into(), format!("✖ {p} — {w}")));
        }
    }
    if !r.unstable.is_empty() || !r.new_after.is_empty() || !r.changed_after.is_empty() {
        l.push(("warn".into(), tf!("⚠ Claude je radio tokom backupa: {} menjano dok je čitano, {} novih i {} promenjenih posle kopiranja.",
            "⚠ Claude was working during the backup: {} changed while being read, {} new and {} changed after copying.",
            num(r.unstable.len() as u64), num(r.new_after.len() as u64), num(r.changed_after.len() as u64))));
    }
    if !r.problems.is_empty() {
        l.push(("warn".into(), tf!("⚠ {} pri skeniranju (spisak je u {}).", "⚠ {} while scanning (listed in {}).",
            n_warnings(r.problems.len() as u64), crate::i18n::names().report)));
    }
    l
}
