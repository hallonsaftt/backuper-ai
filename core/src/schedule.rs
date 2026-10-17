//! Automatski backup: na macOS-u preko launchd-a (ista oznaka kao Python verzija, pa jedno zamenjuje drugo),
//! na Windowsu preko Task Scheduler-a. U oba slučaja propušten termin se nadoknadi čim se računar upali.

use crate::env::{uid, Env};
use crate::scan::load_json;
use crate::i18n::t;
use crate::tf;
use crate::util::*;
use chrono::{Datelike, Duration as CDuration, Local, NaiveDateTime, TimeZone};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SCHED_LABEL: &str = "com.claudebackup.auto";

pub fn plist_path(env: &Env) -> PathBuf {
    env.home.join(format!("Library/LaunchAgents/{SCHED_LABEL}.plist"))
}
pub fn log_path(env: &Env) -> PathBuf {
    if cfg!(windows) {
        return env.support.join("claude-backup.log");
    }
    env.home.join("Library/Logs/claude-backup.log")
}
/// Windows: zadatak u Task Scheduler-u i naš opis podešavanja (Task Scheduler ga ne čuva u čitljivom obliku)
pub const WIN_TASK: &str = "ClaudeBackup\\Auto backup";
/// name used by the first Windows build — still recognised and cleaned up
pub const WIN_TASK_OLD: &str = "ClaudeBackup\\Automatski backup";

fn win_task_exists(name: &str) -> bool {
    command("schtasks").args(["/Query", "/TN", name]).output().map(|o| o.status.success()).unwrap_or(false)
}
pub fn win_config_path(env: &Env) -> PathBuf {
    env.support.join("schedule.json")
}
pub fn last_run_path(env: &Env) -> PathBuf {
    env.support.join("last_run.json")
}
pub fn force_flag_path(env: &Env) -> PathBuf {
    env.support.join("run-now.flag")
}

pub fn launchctl(args: &[&str]) -> (i32, String) {
    match command("launchctl").args(args).output() {
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
        "daily" => t("svakog dana", "every day"),
        "weekly" => t("svakog ponedeljka", "every Monday"),
        _ => t("1. u mesecu", "on the 1st of the month"),
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
    if cfg!(windows) {
        let cp = win_config_path(env);
        if !cp.is_file() {
            return st;
        }
        st.enabled = true;
        match load_json(&cp) {
            Some(v) => {
                st.freq = v["freq"].as_str().unwrap_or("daily").into();
                st.hour = v["hour"].as_u64().unwrap_or(0) as u32;
                st.minute = v["minute"].as_u64().unwrap_or(0) as u32;
                st.dest = v["dest"].as_str().unwrap_or("").into();
                st.dest_tilde = env.tilde(Path::new(&st.dest));
                st.keep = v["keep"].as_u64().unwrap_or(0) as u32;
                st.program = v["program"].as_str().unwrap_or("").into();
            }
            None => st.broken = true,
        }
        st.loaded = win_task_exists(WIN_TASK) || win_task_exists(WIN_TASK_OLD);
        st.describe = describe(&st);
        return st;
    }
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
        t("čuva sve", "keeps all").to_string()
    } else {
        tf!("čuva {}", "keeps the last {}", pl(st.keep as u64, ("poslednji backup", "poslednja backupa", "poslednjih backupa"), ("backup", "backups")))
    };
    tf!("{} u {:02}:{:02} → {} · {}", "{} at {:02}:{:02} → {} · {}", freq_text(&st.freq), st.hour, st.minute, st.dest_tilde, keep)
}

/// Pravi macOS LaunchAgent koji pokreće OVU aplikaciju u pozadini (bez prozora).
/// RunAtLoad + --if-due: ako je termin propušten (Mac bio ugašen), backup se napravi čim se uloguješ.
pub fn install_schedule(env: &Env, program: &Path, freq: &str, hour: u32, minute: u32, dest: &Path, keep: u32, html: bool) -> Result<(), String> {
    if cfg!(windows) {
        return win_install(env, program, freq, hour, minute, dest, keep, html);
    }
    if !cfg!(target_os = "macos") && std::env::var("CLAUDE_BACKUP_FAKE_MAC").is_err() {
        return Err(t("Automatski backup preko launchd radi samo na macOS-u.", "Automatic backup via launchd only works on macOS.").into());
    }
    let ps = program.to_string_lossy();
    if ps.starts_with("/Volumes/") || ps.contains("/AppTranslocation/") {
        return Err(t("Prvo prevuci Claude Backup u folder Applications (Programi), pa ga otvori odande — \
                      zakazivanje mora da zna stalnu lokaciju aplikacije.",
                     "First drag Claude Backup into the Applications folder and open it from there — \
                      scheduling needs to know the app's permanent location.").into());
    }
    let p = plist_path(env);
    let log = log_path(env);
    for d in [dest, &env.support, log.parent().unwrap(), p.parent().unwrap()] {
        fs::create_dir_all(d).map_err(|e| tf!("Ne mogu da napravim {}: {}", "Can't create {}: {}", env.tilde(d), os_err(&e)))?;
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
    plist::Value::Dictionary(d).to_file_xml(&p).map_err(|e| tf!("Ne mogu da upišem {}: {}", "Can't write {}: {}", env.tilde(&p), e))?;
    let target = format!("gui/{}", uid());
    launchctl(&["bootout", &format!("{target}/{SCHED_LABEL}")]);
    let (mut rc, mut err) = launchctl(&["bootstrap", &target, &p.to_string_lossy()]);
    if rc != 0 {
        (rc, err) = launchctl(&["load", "-w", &p.to_string_lossy()]);
    }
    if rc != 0 {
        return Err(tf!("macOS nije prihvatio zakazivanje: {}", "macOS didn't accept the schedule: {}", err));
    }
    Ok(())
}

pub fn remove_schedule(env: &Env) -> Result<String, String> {
    if cfg!(windows) {
        for name in [WIN_TASK, WIN_TASK_OLD] {
            if win_task_exists(name) {
                let o = command("schtasks").args(["/Delete", "/TN", name, "/F"]).output().map_err(|e| e.to_string())?;
                if !o.status.success() {
                    return Err(tf!("Windows nije uklonio zadatak: {}", "Windows didn't remove the task: {}", String::from_utf8_lossy(&o.stderr).trim()));
                }
            }
        }
        let cp = win_config_path(env);
        if cp.is_file() {
            fs::remove_file(&cp).map_err(|e| os_err(&e))?; // samo naš opis podešavanja
        }
        return Ok(t("Automatski backup je isključen. Postojeći backupi su ostali gde jesu.", "Automatic backup is off. Existing backups stay where they are.").into());
    }
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
            return Err(t("Fajl zakazivanja nije napravio ovaj program — ne diram ga.", "That schedule file wasn't made by this program — leaving it alone.").into());
        }
        fs::remove_file(&p).map_err(|e| tf!("Ne mogu da uklonim {}: {}", "Can't remove {}: {}", env.tilde(&p), os_err(&e)))?; // samo naš fajl
    }
    Ok(t("Automatski backup je isključen. Postojeći backupi su ostali gde jesu.", "Automatic backup is off. Existing backups stay where they are.").into())
}

pub fn run_schedule_now(env: &Env) -> Result<(), String> {
    let _ = fs::create_dir_all(&env.support);
    let _ = fs::write(force_flag_path(env), "1"); // znak da ovo pokretanje ne čeka termin
    if cfg!(windows) {
        let name = if win_task_exists(WIN_TASK) { WIN_TASK } else { WIN_TASK_OLD };
        let o = command("schtasks").args(["/Run", "/TN", name]).output().map_err(|e| e.to_string())?;
        return if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) };
    }
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
        return (true, t("nema podataka o zakazivanju — pravim backup", "no schedule info — making a backup").into());
    }
    let slot = last_slot(&st.freq, st.hour, st.minute, now);
    let installed = fs::metadata(if cfg!(windows) { win_config_path(env) } else { plist_path(env) })
        .and_then(|m| m.modified())
        .ok()
        .map(|t| chrono::DateTime::<Local>::from(t).naive_local())
        .unwrap_or(NaiveDateTime::MIN);
    if slot < installed {
        return (false, t("od uključivanja još nije bilo termina — preskačem", "no scheduled time has passed since it was turned on — skipping").into());
    }
    if let Some(lr) = &st.last_run {
        if lr.ok {
            if let Some(t) = parse_ts(&lr.time) {
                if t.naive_local() >= slot {
                    return (false, tf!("backup za termin {} je već napravljen — preskačem", "the backup for {} is already done — skipping", slot.format(crate::i18n::t("%d.%m.%Y. %H:%M", "%b %-d, %Y %H:%M"))));
                }
            }
        }
    }
    (true, tf!("termin {} nema backup — pravim ga", "{} has no backup yet — making it", slot.format(t("%d.%m.%Y. %H:%M", "%b %-d, %Y %H:%M"))))
}

/// Briše najstarije AUTOMATSKE backupe preko `keep`. Dira SAMO ClaudeBackup_*.zip čiji MANIFEST kaže
/// da ih je napravio automatski backup — ručne nikad, kao ni trenutni.
pub fn rotate_backups(dest: &Path, keep: u32, current: &Path) -> Vec<PathBuf> {
    if keep < 1 {
        return vec![];
    }
    let re = regex::Regex::new(r"^ClaudeBackup_\d{4}-\d{2}-\d{2}_\d{6}(_\d+)?\.zip$").unwrap();
    let cur = crate::env::real(current).unwrap_or(current.to_path_buf());
    let mut names: Vec<PathBuf> = match fs::read_dir(dest) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                re.is_match(&n)
                    && fs::symlink_metadata(p).map(|m| m.is_file()).unwrap_or(false)
                    && crate::env::real(p).map(|c| c != cur).unwrap_or(false)
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
    #[cfg(windows)]
    {
        use tauri_winrt_notification::Toast;
        let _ = Toast::new(WIN_APP_ID).title(title).text1(msg).show();
    }
    #[cfg(not(windows))]
    {
        if !cfg!(target_os = "macos") && std::env::var("CLAUDE_BACKUP_FAKE_MAC").is_err() {
            return;
        }
        let _ = command("osascript")
            .args(["-e", "on run argv", "-e", "display notification (item 1 of argv) with title (item 2 of argv)", "-e", "end run", msg, title])
            .output();
    }
}

/// Isti identifikator kao u tauri.conf.json — Windows instaler ga dodeljuje prečici u Start meniju.
pub const WIN_APP_ID: &str = "com.hallonsaftt.claudebackup";

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Windows komandna linija: argument pod navodnicima ako treba (uz pravila za „\” ispred navodnika).
pub fn win_quote(s: &str) -> String {
    if !s.is_empty() && !s.contains([' ', '\t', '"']) {
        return s.to_string();
    }
    let mut out = String::from("\"");
    let mut bs = 0usize;
    for c in s.chars() {
        if c == '\\' {
            bs += 1;
            continue;
        }
        if c == '"' {
            out.push_str(&"\\".repeat(bs * 2 + 1));
        } else {
            out.push_str(&"\\".repeat(bs));
        }
        bs = 0;
        out.push(c);
    }
    out.push_str(&"\\".repeat(bs * 2));
    out.push('"');
    out
}

/// XML za Task Scheduler: termin + pokretanje pri prijavi (sa --if-due) + „pokreni što pre ako je propušteno”.
#[allow(clippy::too_many_arguments)]
pub fn win_task_xml(program: &Path, freq: &str, hour: u32, minute: u32, dest: &Path, keep: u32, html: bool, user: &str, start: &str) -> String {
    let mut args = vec!["--backup".to_string(), "--scheduled".into(), "--if-due".into(), "--notify".into(),
                        "--dest".into(), win_quote(&dest.to_string_lossy()), "--format".into(),
                        (if html { "html" } else { "none" }).into()];
    if keep > 0 {
        args.push("--keep".into());
        args.push(keep.to_string());
    }
    let sched = match freq {
        "weekly" => "<ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek><Monday /></DaysOfWeek></ScheduleByWeek>".to_string(),
        "monthly" => format!("<ScheduleByMonth><DaysOfMonth><Day>1</Day></DaysOfMonth><Months>{}</Months></ScheduleByMonth>",
            ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
                .iter().map(|m| format!("<{m} />")).collect::<String>()),
        _ => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
    };
    let user = xml_esc(user);
    format!(r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>Claude Backup</Author>
    <Description>{desc}</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start}T{hour:02}:{minute:02}:00</StartBoundary>
      <Enabled>true</Enabled>
      {sched}
    </CalendarTrigger>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT2M</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT4H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{cmd}</Command>
      <Arguments>{args}</Arguments>
    </Exec>
  </Actions>
</Task>
"#, cmd = xml_esc(&program.to_string_lossy()), args = xml_esc(&args.join(" ")),
       desc = t("Claude Backup — automatski backup Claude istorije. Isključuje se iz aplikacije.", "Claude Backup — automatic backup of your Claude history. Turn it off from the app."))
}

#[allow(clippy::too_many_arguments)]
fn win_install(env: &Env, program: &Path, freq: &str, hour: u32, minute: u32, dest: &Path, keep: u32, html: bool) -> Result<(), String> {
    for d in [dest, &env.support] {
        fs::create_dir_all(d).map_err(|e| tf!("Ne mogu da napravim {}: {}", "Can't create {}: {}", env.tilde(d), os_err(&e)))?;
    }
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) if !d.is_empty() => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => return Err(t("Ne znam ime Windows korisnika (USERNAME).", "Can't tell the Windows user name (USERNAME).").into()),
    };
    // prvi termin u budućnosti, da Windows odmah po uključivanju ne „nadoknađuje” današnji
    let now = Local::now().naive_local();
    let mut first = now.date().and_hms_opt(hour, minute, 0).unwrap_or(now);
    if first <= now {
        first += CDuration::days(1);
    }
    let start = first.format("%Y-%m-%d").to_string();
    let xml = win_task_xml(program, freq, hour, minute, dest, keep, html, &user, &start);
    // opis podešavanja ide PRE zadatka: ako se zadatak pokrene odmah, --if-due već zna vreme uključivanja
    let cfg = json!({"freq": freq, "hour": hour, "minute": minute, "dest": dest.to_string_lossy(), "keep": keep,
                     "html": html, "program": program.to_string_lossy(), "installed": now_iso_utc()});
    let cfg_path = win_config_path(env);
    fs::write(&cfg_path, serde_json::to_vec_pretty(&cfg).unwrap()).map_err(|e| os_err(&e))?;
    // Task Scheduler najpouzdanije čita UTF-16 sa BOM-om
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let xml_path = env.support.join("zadatak.xml");
    fs::write(&xml_path, &bytes).map_err(|e| os_err(&e))?;
    let out = command("schtasks").args(["/Create", "/TN", WIN_TASK, "/F", "/XML"]).arg(&xml_path).output();
    let _ = fs::remove_file(&xml_path); // naš privremeni fajl
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            let _ = fs::remove_file(&cfg_path);
            return Err(tf!("Ne mogu da pokrenem schtasks: {}", "Can't run schtasks: {}", e));
        }
    };
    if !out.status.success() {
        let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        let _ = fs::remove_file(&cfg_path); // naš opis — zadatak nije napravljen
        return Err(tf!("Windows nije prihvatio zakazivanje: {}", "Windows didn't accept the schedule: {}", msg));
    }
    if win_task_exists(WIN_TASK_OLD) {
        let _ = command("schtasks").args(["/Delete", "/TN", WIN_TASK_OLD, "/F"]).output(); // naš stari zadatak
    }
    Ok(())
}

pub fn local_now_naive() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn naive_from_str(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok().or_else(|| Local.timestamp_opt(0, 0).single().map(|d| d.naive_local()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_windows_args() {
        assert_eq!(win_quote("C:\\x"), "C:\\x");
        assert_eq!(win_quote("C:\\Moji backupi"), "\"C:\\Moji backupi\"");
        assert_eq!(win_quote("D:\\a b\\"), "\"D:\\a b\\\\\"");
        assert_eq!(win_quote("a\"b c"), "\"a\\\"b c\"");
    }

    #[test]
    fn task_xml_has_everything() {
        let x = win_task_xml(Path::new("C:\\Users\\Pera Perić\\AppData\\Local\\Claude Backup\\claude-backup.exe"), "weekly", 14, 5,
                             Path::new("C:\\Users\\Pera Perić\\ClaudeBackups"), 14, true, "PC\\Pera & Co", "2026-10-18");
        assert!(x.contains("<StartBoundary>2026-10-18T14:05:00</StartBoundary>"));
        assert!(x.contains("<Monday />"));
        assert!(x.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(x.contains("<UserId>PC\\Pera &amp; Co</UserId>"));
        assert!(x.contains("--if-due"));
        assert!(x.contains("--dest &quot;C:\\Users\\Pera Perić\\ClaudeBackups&quot;"));
        assert!(x.contains("--keep 14"));
        let m = win_task_xml(Path::new("C:\\a.exe"), "monthly", 3, 0, Path::new("D:\\"), 0, false, "u", "2026-10-18");
        assert!(m.contains("<Day>1</Day>") && m.contains("<December />") && !m.contains("--keep") && m.contains("--format none"));
    }

    #[test]
    fn last_slot_rules() {
        let now = NaiveDateTime::parse_from_str("2026-10-17 13:00", "%Y-%m-%d %H:%M").unwrap();
        assert_eq!(last_slot("daily", 14, 0, now).to_string(), "2026-10-16 14:00:00");
        assert_eq!(last_slot("weekly", 14, 0, now).to_string(), "2026-10-12 14:00:00");
        assert_eq!(last_slot("monthly", 14, 0, now).to_string(), "2026-10-01 14:00:00");
    }
}
