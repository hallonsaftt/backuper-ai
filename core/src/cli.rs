//! Rad bez prozora: koristi ga zakazani backup (launchd / Task Scheduler pokreće aplikaciju sa --backup --scheduled …)
//! i testovi. Opcije su iste kao kod Python verzije.

use crate::backup::*;
use crate::env::{expand, Env};
use crate::restore::*;
use crate::retention::*;
use crate::scan::*;
use crate::schedule::*;
use crate::i18n::t;
use crate::tf;
use crate::util::*;
use crate::verify::*;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
struct Args {
    backup: bool,
    only: Option<String>,
    all: bool,
    project: Vec<String>,
    format: String,
    dest: Option<String>,
    folder: bool,
    zip: bool,
    verify: Option<String>,
    restore: Option<String>,
    yes: bool,
    set_retention: Option<i64>,
    keep: u32,
    notify: bool,
    scheduled: bool,
    if_due: bool,
    schedule: Option<String>,
    at: String,
    unschedule: bool,
    project_backup: Option<String>,
    list: bool,
    lang: Option<String>,
}

const CLI_FLAGS: [&str; 10] = ["--backup", "--verify", "--restore", "--set-retention", "--schedule", "--unschedule",
                               "--project-backup", "--list", "--help", "--version"];

/// Da li argumenti traže rad bez prozora?
pub fn is_cli(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| CLI_FLAGS.contains(&a.as_str()))
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args { format: "html,md".into(), at: "03:00".into(), ..Default::default() };
    let mut i = 1;
    let val = |i: &mut usize| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| tf!("{} traži vrednost", "{} needs a value", args[*i - 1]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--backup" => a.backup = true,
            "--only" => a.only = Some(val(&mut i)?),
            "--all" => a.all = true,
            "--project" => a.project.push(val(&mut i)?),
            "--format" => a.format = val(&mut i)?,
            "--dest" => a.dest = Some(val(&mut i)?),
            "--folder" => a.folder = true,
            "--zip" => a.zip = true,
            "--verify" => a.verify = Some(val(&mut i)?),
            "--restore" => a.restore = Some(val(&mut i)?),
            "--yes" => a.yes = true,
            "--set-retention" => a.set_retention = Some(val(&mut i)?.parse().map_err(|_| t("--set-retention traži broj", "--set-retention needs a number"))?),
            "--keep" => a.keep = val(&mut i)?.parse().map_err(|_| t("--keep traži broj", "--keep needs a number"))?,
            "--notify" => a.notify = true,
            "--scheduled" => a.scheduled = true,
            "--if-due" => a.if_due = true,
            "--schedule" => a.schedule = Some(val(&mut i)?),
            "--at" => a.at = val(&mut i)?,
            "--unschedule" => a.unschedule = true,
            "--project-backup" => {
                let next = args.get(i + 1).cloned().unwrap_or_default();
                if !next.is_empty() && !next.starts_with("--") {
                    i += 1;
                    a.project_backup = Some(next);
                } else {
                    a.project_backup = Some(".".into());
                }
            }
            "--list" => a.list = true,
            "--lang" => a.lang = Some(val(&mut i)?),
            x if x.starts_with("-psn_") => {} // macOS ponekad doda ovo
            other => return Err(tf!("nepoznata opcija: {}", "unknown option: {}", other)),
        }
        i += 1;
    }
    Ok(a)
}

fn print_lines(lines: &[(String, String)]) {
    for (st, t) in lines {
        if st == "head" {
            println!("\n{t}");
        } else {
            println!("{t}");
        }
    }
}

fn formats(f: &str) -> (bool, bool) {
    if f.trim() == "none" {
        return (false, false);
    }
    let parts: Vec<&str> = f.split(',').map(|s| s.trim()).collect();
    (parts.contains(&"html"), parts.contains(&"md"))
}

fn ts() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn run(args: Vec<String>) -> i32 {
    // jezik: --lang en|sr, inače sačuvan izbor iz aplikacije (podrazumevano engleski)
    let env = Env::new();
    crate::i18n::init(&env);
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        if let Some(l) = args.get(i + 1) {
            crate::i18n::set_lang(l);
        }
    }
    if args.iter().any(|a| a == "--help") {
        println!("{}", t(
            "Claude Backup — komandna linija\n\n  --backup [--only a,b] [--all] [--format html,md|none] [--dest FOLDER] [--folder]\n  --project-backup [FOLDER] [--zip]\n  --verify PUTANJA\n  --restore PUTANJA [--yes]\n  --set-retention DANA\n  --schedule daily|weekly|monthly [--at SS:MM] [--keep N] [--dest FOLDER]\n  --unschedule\n  --list\n  --lang en|sr",
            "Claude Backup — command line\n\n  --backup [--only a,b] [--all] [--format html,md|none] [--dest FOLDER] [--folder]\n  --project-backup [FOLDER] [--zip]\n  --verify PATH\n  --restore PATH [--yes]\n  --set-retention DAYS\n  --schedule daily|weekly|monthly [--at HH:MM] [--keep N] [--dest FOLDER]\n  --unschedule\n  --list\n  --lang en|sr"));
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("Claude Backup {TOOL_VERSION}");
        return 0;
    }
    let a = match parse(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("✖ {e}");
            return 1;
        }
    };
    #[cfg(windows)]
    win_stdio(&env, a.scheduled);
    let none = no_progress();

    if a.list {
        let inv = scan(&env);
        println!("{}", t("Claude podaci na ovom računaru", "Claude data on this computer"));
        for c in &inv.cats {
            println!("  {} {:<40} {}   [{}]", if c.files.is_empty() { "–" } else { "✔" }, c.title, cat_note(&inv, &c.id, None), c.id);
        }
        println!("\n{}", t("Projekti", "Projects"));
        let mut ps: Vec<&Proj> = inv.projects.values().collect();
        ps.sort_by(|x, y| y.last.partial_cmp(&x.last).unwrap_or(std::cmp::Ordering::Equal));
        for p in ps {
            println!("  {}  —  {}, {}   [{}]", if p.cwd.is_empty() { p.key.clone() } else { env.tilde(Path::new(&p.cwd)) }, n_convs(p.sessions), human(p.size), p.key);
        }
        let r = retention_info(&env, &inv.cc_version);
        println!("\n{}\n  cleanupPeriodDays: {}   Claude Code: {}", t("Čuvanje istorije", "History retention"),
                 r.cleanup.map(|c| c.to_string()).unwrap_or(t("nije podešeno (30)", "not set (30)").into()),
                 if inv.cc_version.is_empty() { "?" } else { &inv.cc_version });
        let st = schedule_status(&env);
        if st.enabled {
            println!("\n{}\n  {}", t("Automatski backup", "Automatic backup"), st.describe);
        }
        return 0;
    }
    if a.unschedule {
        return match remove_schedule(&env) {
            Ok(m) => {
                println!("✔ {m}");
                0
            }
            Err(e) => {
                println!("✖ {e}");
                1
            }
        };
    }
    if let Some(freq) = &a.schedule {
        let (h, m) = match a.at.split_once(':') {
            Some((h, m)) => (h.parse::<u32>().unwrap_or(99), m.parse::<u32>().unwrap_or(99)),
            None => (99, 99),
        };
        if h > 23 || m > 59 || !["daily", "weekly", "monthly"].contains(&freq.as_str()) {
            println!("✖ --schedule daily|weekly|monthly, --at SS:MM");
            return 1;
        }
        let dest = PathBuf::from(expand(a.dest.as_deref().unwrap_or("~/ClaudeBackups"), &env.home));
        let exe = std::env::current_exe().unwrap_or_default();
        return match install_schedule(&env, &exe, freq, h, m, &dest, a.keep, true) {
            Ok(()) => {
                println!("✔ {}", schedule_status(&env).describe);
                0
            }
            Err(e) => {
                println!("✖ {e}");
                1
            }
        };
    }
    if let Some(days) = a.set_retention {
        let r = set_retention(&env, days);
        println!("{} {}", if r.ok { "✔" } else { "✖" }, r.message);
        for c in &r.changes {
            println!("  {c}");
        }
        if !r.backup.is_empty() {
            println!("  {} {}", t("stari fajl:", "old file:"), r.backup);
        }
        if !(a.backup || a.verify.is_some() || a.restore.is_some()) {
            return if r.ok { 0 } else { 1 };
        }
    }
    if let Some(p) = &a.verify {
        let p = PathBuf::from(expand(p, &env.home));
        let vr = verify_backup(&p, &none);
        if vr.passed() {
            println!("{}", tf!("✔ Backup je ispravan\n✔ Svih {} ({}) je identično onome što je sačuvano (SHA-256).",
                               "✔ The backup is intact\n✔ All {} ({}) are identical to what was saved (SHA-256).", n_files(vr.ok), human(vr.bytes)));
            return 0;
        }
        if !vr.error.is_empty() {
            println!("{}", tf!("✖ Backup ne može da se pročita\n✖ {}", "✖ The backup can't be read\n✖ {}", vr.error));
        } else {
            println!("{}", tf!("✖ Backup nije ispravan\n✖ Ispravno: {} od {}.", "✖ The backup is NOT intact\n✖ Intact: {} of {}.", num(vr.ok), num(vr.total)));
            for b in vr.bad.iter().take(6) {
                println!("  {} {b}", t("oštećen:", "damaged:"));
            }
            for b in vr.missing.iter().take(6) {
                println!("  {} {b}", t("nedostaje:", "missing:"));
            }
        }
        return 2;
    }
    if let Some(p) = &a.restore {
        let p = PathBuf::from(expand(p, &env.home));
        let vr = verify_backup(&p, &none);
        if !vr.passed() {
            println!("{}", tf!("✖ Backup nije ispravan — vraćanje otkazano. {}", "✖ The backup is not intact — restore cancelled. {}", vr.error));
            return 2;
        }
        let mut items = plan_restore(&env, vr.manifest.as_ref().unwrap(), &none);
        let only: Option<Vec<String>> = a.only.as_ref().map(|o| o.split(',').map(|s| s.trim().to_string()).collect());
        if let Some(o) = &only {
            items.retain(|i| o.iter().any(|c| Some(c.as_str()) == i.e["cat"].as_str()));
        }
        let cnt = |s: &str| items.iter().filter(|i| i.status == s).count();
        println!("{}", tf!("Za vraćanje: {} · isto: {} · drugačije (ostaje tvoje): {} · bez projekta: {} · odbijeno: {}",
                           "To restore: {} · same: {} · different (yours stays): {} · no project: {} · refused: {}",
                           cnt("add"), cnt("same"), cnt("diff"), cnt("noproj"), cnt("unsafe")));
        if !a.yes {
            println!("{}", t("Ništa nije promenjeno. Dodaj --yes da vratiš fajlove koji fale.", "Nothing was changed. Add --yes to restore the missing files."));
            return 0;
        }
        let mut rd = match Reader::open(&p) {
            Ok(r) => r,
            Err(e) => {
                println!("✖ {e}");
                return 2;
            }
        };
        let r = apply_restore(&mut rd, &mut items, None, &none);
        println!("{}", tf!("✔ Vraćeno: {}", "✔ Restored: {}", n_files(r.added)));
        for (s, w) in &r.failed {
            println!("✖ {s} — {w}");
        }
        return if r.failed.is_empty() { 0 } else { 1 };
    }
    if let Some(pb) = &a.project_backup {
        let inv = scan(&env);
        let target = std::fs::canonicalize(expand(pb, &env.home)).unwrap_or(PathBuf::from(pb));
        let pr = match project_for_path(&inv, &target) {
            Some(p) => p.clone(),
            None => {
                println!("{}", tf!("✖ {} nije Claude Code projekat (nema razgovora za taj folder).", "✖ {} is not a Claude Code project (no conversations for that folder).", env.tilde(&target)));
                return 1;
            }
        };
        let dest = project_dest(Path::new(&pr.cwd));
        if let Err(e) = prepare_project_dest(&dest) {
            println!("{}", tf!("✖ Ne mogu da pišem u {}: {}", "✖ Can't write to {}: {}", env.tilde(&dest), e));
            return 1;
        }
        let (html, md) = formats(&a.format);
        let prompts = project_prompt_history(&env, &pr.cwd);
        let name = Path::new(&pr.cwd).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let o = BackupOpts {
            cats: vec![],
            projects: Some(HashSet::from([pr.key.clone()])),
            html,
            md,
            dest,
            as_zip: a.zip,
            prefix: format!("ClaudeBackup_{}", slug(&name, 30)),
            project_key: Some(pr.key.clone()),
            scope: json!({"type": "project", "cwd": pr.cwd, "key": pr.key}),
            extra: if prompts.is_empty() { vec![] } else { vec![(crate::i18n::names().prompts.into(), prompts)] },
        };
        return match run_backup(&inv, &o, &none, None) {
            Ok(r) => {
                println!("{}", tf!("Istorija projekta {}", "Project history {}", env.tilde(Path::new(&pr.cwd))));
                print_lines(&report_lines(&r));
                if r.verify.passed() { 0 } else { 2 }
            }
            Err(e) => {
                println!("{}", tf!("✖ Backup nije uspeo: {}", "✖ Backup failed: {}", os_err(&e)));
                1
            }
        };
    }
    if a.backup {
        if a.scheduled && a.if_due {
            let flag = force_flag_path(&env);
            if flag.is_file() {
                let _ = std::fs::remove_file(&flag); // naš znak za „pokreni sada”
            } else {
                let (due, why) = schedule_due(&env, None);
                println!("[{}] {}", ts(), why);
                if !due {
                    return 0;
                }
            }
        }
        let dest = PathBuf::from(expand(a.dest.as_deref().unwrap_or("~/Desktop"), &env.home));
        if a.scheduled {
            println!("\n[{}] {} → {}", ts(), t("Automatski backup", "Automatic backup"), dest.display());
            if !dest.is_dir() && dest.parent().map(|p| p.is_dir()).unwrap_or(false) && !dest.starts_with("/Volumes/") {
                let _ = std::fs::create_dir_all(&dest);
            }
        }
        let mut err = dest_ok(&env, &dest);
        if err.is_some() && dest.starts_with("/Volumes/") {
            let vol: PathBuf = dest.components().take(3).collect();
            if !vol.exists() {
                err = Some(tf!("Spoljni disk „{}” nije prikačen.", "External drive “{}” isn't connected.", vol.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
            }
        }
        if let Some(e) = err {
            println!("✖ {e} ({})", dest.display());
            if a.notify {
                notify(t("Claude Backup nije napravljen", "Claude Backup was not made"), &format!("{e} {}", env.tilde(&dest)));
            }
            if a.scheduled {
                write_last_run(&env, false, &tf!("nije napravljen: {} ({})", "not made: {} ({})", e, env.tilde(&dest)), "");
            }
            return 1;
        }
        let inv = scan(&env);
        let cats: Vec<String> = match &a.only {
            Some(o) => o.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            None => inv.cats.iter().filter(|c| !c.files.is_empty() && (c.default || a.all)).map(|c| c.id.clone()).collect(),
        };
        if let Some(bad) = cats.iter().find(|c| !CAT_DEFS.iter().any(|d| d.id == c.as_str())) {
            println!("{}", tf!("✖ Nepoznata kategorija: {}", "✖ Unknown category: {}", bad));
            return 1;
        }
        let (html, md) = formats(&a.format);
        let mut o = BackupOpts::full(cats, dest.clone());
        o.html = html;
        o.md = md;
        o.as_zip = !a.folder;
        if !a.project.is_empty() {
            o.projects = Some(a.project.iter().cloned().collect());
        }
        if a.scheduled {
            o.scope = json!({"type": "full", "auto": true});
        }
        if !inv.running.is_empty() {
            println!("{}", tf!("⚠ Upaljeno: {} — za najtačniji backup zatvori Claude.", "⚠ Running: {} — quit Claude for the most exact backup.", inv.running.join(", ")));
        }
        let r = match run_backup(&inv, &o, &none, None) {
            Ok(r) => r,
            Err(e) => {
                let m = os_err(&e);
                println!("{}", tf!("✖ Backup nije uspeo: {}", "✖ Backup failed: {}", m));
                if a.notify {
                    notify(t("Claude Backup nije uspeo", "Claude Backup failed"), &m);
                }
                if a.scheduled {
                    write_last_run(&env, false, &tf!("nije uspeo: {}", "failed: {}", m), "");
                }
                return 1;
            }
        };
        print_lines(&report_lines(&r));
        let mut removed = vec![];
        if a.keep > 0 && r.verify.passed() && o.as_zip {
            removed = rotate_backups(&dest, a.keep, Path::new(&r.path));
            for p in &removed {
                println!("  {} {}", t("obrisan stari automatski backup:", "removed old automatic backup:"), p.file_name().unwrap_or_default().to_string_lossy());
            }
        }
        let n = r.per_cat.iter().find(|c| c.id == "code_sessions").map(|_| {
            inv.cat("code_sessions").files.iter().filter(|f| f.is_session_file()).count() as u64
        }).unwrap_or(0);
        let mut warn = vec![];
        if !r.failed.is_empty() {
            warn.push(tf!("{} preskočeno", "{} skipped", n_files(r.failed.len() as u64)));
        }
        if !r.problems.is_empty() {
            warn.push(tf!("{} (vidi {})", "{} (see {})", n_warnings(r.problems.len() as u64), crate::i18n::names().report));
        }
        let summary = if r.verify.passed() {
            let mut s = tf!("{} · {} · proveren", "{} · {} · verified", n_convs(n), human(r.size));
            if !warn.is_empty() {
                s.push_str(&format!(" · {}", warn.join(", ")));
            }
            if !removed.is_empty() {
                s.push_str(&tf!(" · obrisano starih: {}", " · old ones removed: {}", removed.len()));
            }
            s
        } else {
            tf!("provera nije prošla: ispravno {} od {}", "verification failed: {} of {} intact", r.verify.ok, r.verify.total)
        };
        if a.notify {
            let title = if r.verify.passed() && warn.is_empty() { "Claude Backup ✔" } else if r.verify.passed() { "Claude Backup ⚠" } else { "Claude Backup ✖" };
            notify(title, &summary);
        }
        if a.scheduled {
            write_last_run(&env, r.verify.passed(), &summary, &r.path);
        }
        return if r.verify.passed() { 0 } else { 2 };
    }
    println!("{}", t("Nema komande. Probaj --help.", "No command. Try --help."));
    1
}

/// Windows: aplikacija nema konzolu. Ručno pokrenuta iz terminala — piše u taj terminal;
/// pokrenuta iz Task Scheduler-a — piše u dnevnik (claude-backup.log).
#[cfg(windows)]
fn win_stdio(env: &Env, scheduled: bool) {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
    unsafe {
        if scheduled {
            let log = log_path(env);
            if let Some(d) = log.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if std::fs::metadata(&log).map(|m| m.len() > 5_000_000).unwrap_or(false) {
                let _ = std::fs::rename(&log, log.with_extension("log.old")); // naš dnevnik, da ne raste bez kraja
            }
            if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
                let h = f.into_raw_handle();
                SetStdHandle(STD_OUTPUT_HANDLE, h as _);
                SetStdHandle(STD_ERROR_HANDLE, h as _);
            }
        } else {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if h.is_null() || h == INVALID_HANDLE_VALUE {
                AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
    }
}
