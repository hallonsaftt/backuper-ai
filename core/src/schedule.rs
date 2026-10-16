//! Automatski backup preko macOS planera (launchd). Ista oznaka kao Python verzija, pa jedno zamenjuje drugo.

use crate::env::{uid, Env};
use crate::scan::load_json;
use crate::util::*;
use chrono::{Datelike, Duration as CDuration, Local, NaiveDateTime, TimeZone};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SCHED_LABEL: &str = "com.claudebackup.auto";

pub fn plist_path(env: &Env) -> PathBuf {
    env.home.join(format!("Library/LaunchAgents/{SCHED_LABEL}.plist"))
}
pub fn log_path(env: &Env) -> PathBuf {
    env.home.join("Library/Logs/claude-backup.log")
}
pub fn last_run_path(env: &Env) -> PathBuf {
    env.support.join("last_run.json")
}
pub fn force_flag_path(env: &Env) -> PathBuf {
    env.support.join("run-now.flag")
}

pub fn launchctl(args: &[&str]) -> (i32, String) {
    match Command::new("launchctl").args(args).output() {
        Ok(o) => {
            let s = if o.stderr.is_empty() { o.stdout } else { o.stderr };
            (o.status.code().unwrap_or(1), String::from_utf8_lossy(&s).trim().to_string())
        }
        Err(e) => (1, e.to_string()),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SchedStatus {
    pub enabled: bool,
    pub broken: bool,
    pub freq: String,
    pub hour: u32,
    pub minute: u32,
    pub dest: String,
    pub dest_tilde: String,
    pub keep: u32,
    pub loaded: bool,
    pub program: String,
    pub describe: String,
    pub last_run: Option<LastRun>,
    pub log: String,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct LastRun {
    pub time: String,
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub path: String,
}

pub fn freq_text(freq: &str) -> &'static str {
    match freq {
        "daily" => "svakog dana",
        "weekly" => "svakog ponedeljka",
        _ => "1. u mesecu",
    }
}

pub fn read_last_run(env: &Env) -> Option<LastRun> {
    load_json(&last_run_path(env)).and_then(|v| serde_json::from_value(v).ok())
}

pub fn write_last_run(env: &Env, ok: bool, message: &str, path: &str) {
    let p = last_run_path(env);
    let _ = fs::create_dir_all(&env.support);
    let tmp = p.with_file_name("last_run.json.tmp");
    let v = json!({"time": now_iso_utc(), "ok": ok, "message": message, "path": path});
    if fs::write(&tmp, serde_json::to_vec(&v).unwrap()).is_ok() {
        let _ = fs::rename(&tmp, &p);
    }
}

pub fn schedule_status(env: &Env) -> SchedStatus {
    let p = plist_path(env);
    let mut st = SchedStatus {
        enabled: false, broken: false, freq: "daily".into(), hour: 0, minute: 0, dest: String::new(),
        dest_tilde: String::new(), keep: 0, loaded: false, program: String::new(), describe: String::new(),
        last_run: read_last_run(env), log: env.tilde(&log_path(env)),
    };
    if !p.is_file() {
        return st;
    }
    st.enabled = true;
    let v: plist::Value = match plist::Value::from_file(&p) {
        Ok(v) => v,
        Err(_) => {
            st.broken = true;
            return st;
        }
    };
    let d = match v.as_dictionary() {
        Some(d) => d,
        None => {
            st.broken = true;
            return st;
        }
    };
    let args: Vec<String> = d
        .get("ProgramArguments")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_string().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let arg = |name: &str| -> String {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default()
    };
    let cal = d.get("StartCalendarInterval").and_then(|c| c.as_dictionary());
    let geti = |k: &str| cal.and_then(|c| c.get(k)).and_then(|x| x.as_signed_integer()).unwrap_or(0) as u32;
    st.freq = match cal {
        Some(c) if c.contains_key("Day") => "monthly",
        Some(c) if c.contains_key("Weekday") => "weekly",
        _ => "daily",
    }
    .into();
    st.hour = geti("Hour");
    st.minute = geti("Minute");
    st.dest = arg("--dest");
    st.dest_tilde = env.tilde(Path::new(&st.dest));
    st.keep = arg("--keep").parse().unwrap_or(0);
    st.program = args.first().cloned().unwrap_or_default();
    st.loaded = cfg!(target_os = "macos") && launchctl(&["print", &format!("gui/{}/{}", uid(), SCHED_LABEL)]).0 == 0;
    st.describe = describe(&st);
    st
}

pub fn describe(st: &SchedStatus) -> String {
    let keep = if st.keep == 0 {
        "čuva sve".to_string()
    } else {
        format!("čuva {}", plural(st.keep as u64, "poslednji backup", "poslednja backupa", "poslednjih backupa"))
    };
    format!("{} u {:02}:{:02} → {} · {}", freq_text(&st.freq), st.hour, st.minute, st.dest_tilde, keep)
}

/// Pravi macOS LaunchAgent koji pokreće OVU aplikaciju u pozadini (bez prozora).
/// RunAtLoad + --if-due: ako je termin propušten (Mac bio ugašen), backup se napravi čim se uloguješ.
pub fn install_schedule(env: &Env, program: &Path, freq: &str, hour: u32, minute: u32, dest: &Path, keep: u32, html: bool) -> Result<(), String> {
    if !cfg!(target_os = "macos") && std::env::var("CLAUDE_BACKUP_FAKE_MAC").is_err() {
        return Err("Automatski backup preko launchd radi samo na macOS-u.".into());
    }
    let ps = program.to_string_lossy();
    if ps.starts_with("/Volumes/") || ps.contains("/AppTranslocation/") {
        return Err("Prvo prevuci Claude Backup u folder Applications (Programi), pa ga otvori odande — \
                    zakazivanje mora da zna stalnu lokaciju aplikacije.".into());
    }
    let p = plist_path(env);
    let log = log_path(env);
    for d in [dest, &env.support, log.parent().unwrap(), p.parent().unwrap()] {
        fs::create_dir_all(d).map_err(|e| format!("Ne mogu da napravim {}: {}", env.tilde(d), os_err(&e)))?;
    }
    let mut args: Vec<plist::Value> = vec![
        ps.to_string().into(), "--backup".into(), "--scheduled".into(), "--if-due".into(), "--notify".into(),
        "--dest".into(), dest.to_string_lossy().to_string().into(), "--format".into(),
        (if html { "html" } else { "none" }).into(),
    ];
    if keep > 0 {
        args.push("--keep".into());
        args.push(keep.to_string().into());
    }
    let mut cal = plist::Dictionary::new();
    cal.insert("Hour".into(), (hour as i64).into());
    cal.insert("Minute".into(), (minute as i64).into());
    match freq {
        "weekly" => {
            cal.insert("Weekday".into(), 1i64.into());
        }
        "monthly" => {
            cal.insert("Day".into(), 1i64.into());
        }
        _ => {}
    }
    let mut envd = plist::Dictionary::new();
    envd.insert("LANG".into(), "en_US.UTF-8".into());
    let mut d = plist::Dictionary::new();
    d.insert("Label".into(), SCHED_LABEL.into());
    d.insert("ProgramArguments".into(), plist::Value::Array(args));
    d.insert("StartCalendarInterval".into(), plist::Value::Dictionary(cal));
    d.insert("StandardOutPath".into(), log.to_string_lossy().to_string().into());
    d.insert("StandardErrorPath".into(), log.to_string_lossy().to_string().into());
    d.insert("ProcessType".into(), "Background".into());
    d.insert("LowPriorityIO".into(), true.into());
    d.insert("Nice".into(), 10i64.into());
    d.insert("RunAtLoad".into(), true.into());
    d.insert("EnvironmentVariables".into(), plist::Value::Dictionary(envd));
    plist::Value::Dictionary(d).to_file_xml(&p).map_err(|e| format!("Ne mogu da upišem {}: {e}", env.tilde(&p)))?;
    let target = format!("gui/{}", uid());
    launchctl(&["bootout", &format!("{target}/{SCHED_LABEL}")]);
    let (mut rc, mut err) = launchctl(&["bootstrap", &target, &p.to_string_lossy()]);
    if rc != 0 {
        (rc, err) = launchctl(&["load", "-w", &p.to_string_lossy()]);
    }
    if rc != 0 {
        return Err(format!("macOS nije prihvatio zakazivanje: {err}"));
    }
    Ok(())
}

pub fn remove_schedule(env: &Env) -> Result<String, String> {
    let p = plist_path(env);
    let (rc, _) = launchctl(&["bootout", &format!("gui/{}/{}", uid(), SCHED_LABEL)]);
    if rc != 0 {
        launchctl(&["unload", "-w", &p.to_string_lossy()]);
    }
    if p.is_file() {
        let ours = plist::Value::from_file(&p)
            .ok()
            .and_then(|v| v.as_dictionary().and_then(|d| d.get("Label")).and_then(|l| l.as_string()).map(|s| s == SCHED_LABEL))
            .unwrap_or(false);
        if !ours {
            return Err("Fajl zakazivanja nije napravio ovaj program — ne diram ga.".into());
        }
        fs::remove_file(&p).map_err(|e| format!("Ne mogu da uklonim {}: {}", env.tilde(&p), os_err(&e)))?; // samo naš fajl
    }
    Ok("Automatski backup je isključen. Postojeći backupi su ostali gde jesu.".into())
}

pub fn run_schedule_now(env: &Env) -> Result<(), String> {
    let _ = fs::create_dir_all(&env.support);
    let _ = fs::write(force_flag_path(env), "1"); // znak da ovo pokretanje ne čeka termin
    let (rc, err) = launchctl(&["kickstart", "-k", &format!("gui/{}/{}", uid(), SCHED_LABEL)]);
    if rc == 0 { Ok(()) } else { Err(err) }
}

/// Poslednji zakazani termin koji je već prošao.
pub fn last_slot(freq: &str, hour: u32, minute: u32, now: NaiveDateTime) -> NaiveDateTime {
    let mut c = now.date().and_hms_opt(hour, minute, 0).unwrap_or(now);
    match freq {
        "daily" => {
            if c > now {
                c -= CDuration::days(1);
            }
        }
        "weekly" => {
            c -= CDuration::days(c.weekday().num_days_from_monday() as i64);
            if c > now {
                c -= CDuration::days(7);
            }
        }
        _ => {
            c = c.with_day(1).unwrap_or(c);
            if c > now {
                let prev = c - CDuration::days(1);
                c = prev.with_day(1).unwrap_or(prev);
            }
        }
    }
    c
}

/// Da li treba napraviti backup: da, ako je prošao termin (posle uključivanja) za koji još nema uspešnog backupa.
pub fn schedule_due(env: &Env, now: Option<NaiveDateTime>) -> (bool, String) {
    let now = now.unwrap_or_else(|| Local::now().naive_local());
    let st = schedule_status(env);
    if !st.enabled || st.broken {
        return (true, "nema podataka o zakazivanju — pravim backup".into());
    }
    let slot = last_slot(&st.freq, st.hour, st.minute, now);
    let installed = fs::metadata(plist_path(env))
        .and_then(|m| m.modified())
        .ok()
        .map(|t| chrono::DateTime::<Local>::from(t).naive_local())
        .unwrap_or(NaiveDateTime::MIN);
    if slot < installed {
        return (false, "od uključivanja još nije bilo termina — preskačem".into());
    }
    if let Some(lr) = &st.last_run {
        if lr.ok {
            if let Some(t) = parse_ts(&lr.time) {
                if t.naive_local() >= slot {
                    return (false, format!("backup za termin {} je već napravljen — preskačem", slot.format("%d.%m.%Y. %H:%M")));
                }
            }
        }
    }
    (true, format!("termin {} nema backup — pravim ga", slot.format("%d.%m.%Y. %H:%M")))
}

/// Briše najstarije AUTOMATSKE backupe preko `keep`. Dira SAMO ClaudeBackup_*.zip čiji MANIFEST kaže
/// da ih je napravio automatski backup — ručne nikad, kao ni trenutni.
pub fn rotate_backups(dest: &Path, keep: u32, current: &Path) -> Vec<PathBuf> {
    if keep < 1 {
        return vec![];
    }
    let re = regex::Regex::new(r"^ClaudeBackup_\d{4}-\d{2}-\d{2}_\d{6}(_\d+)?\.zip$").unwrap();
    let cur = fs::canonicalize(current).unwrap_or(current.to_path_buf());
    let mut names: Vec<PathBuf> = match fs::read_dir(dest) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                re.is_match(&n)
                    && fs::symlink_metadata(p).map(|m| m.is_file()).unwrap_or(false)
                    && fs::canonicalize(p).map(|c| c != cur).unwrap_or(false)
            })
            .collect(),
        Err(_) => return vec![],
    };
    names.sort();
    let auto: Vec<PathBuf> = names
        .into_iter()
        .filter(|p| {
            let f = match fs::File::open(p) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let mut z = match zip::ZipArchive::new(f) {
                Ok(z) => z,
                Err(_) => return false,
            };
            let man = z.file_names().find(|n| n.ends_with("/MANIFEST.json") && n.matches('/').count() == 1).map(|s| s.to_string());
            let man = match man {
                Some(m) => m,
                None => return false,
            };
            let mut s = String::new();
            if z.by_name(&man).map(|mut r| r.read_to_string(&mut s)).is_err() {
                return false;
            }
            let v: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
            v["format"] == "claude-backup" && v["scope"]["auto"] == true
        })
        .collect();
    let mut removed = vec![];
    let n = auto.len().saturating_sub(keep as usize - 1);
    for p in auto.into_iter().take(n) {
        if fs::remove_file(&p).is_ok() {
            removed.push(p); // samo stari automatski backup ovog programa
        }
    }
    removed
}

pub fn notify(title: &str, msg: &str) {
    if !cfg!(target_os = "macos") && std::env::var("CLAUDE_BACKUP_FAKE_MAC").is_err() {
        return;
    }
    let _ = Command::new("osascript")
        .args(["-e", "on run argv", "-e", "display notification (item 1 of argv) with title (item 2 of argv)", "-e", "end run", msg, title])
        .output();
}

pub fn local_now_naive() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn naive_from_str(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok().or_else(|| Local.timestamp_opt(0, 0).single().map(|d| d.naive_local()))
}
