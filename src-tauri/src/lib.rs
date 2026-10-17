//! Tauri sloj: komande koje poziva interfejs. Sav posao radi `backup-core`.

use backup_core::backup::{dest_ok, run_backup, BackupOpts, BackupResult};
use backup_core::env::{expand, Env};
use backup_core::restore::{apply_restore, plan_restore, summarize, CatPlan, RItem, RestoreResult};
use backup_core::retention::{retention_info, set_retention as core_set_retention, RetentionInfo, SetResult};
use backup_core::scan::*;
use backup_core::schedule::*;
use backup_core::i18n::t;
use backup_core::tf;
use backup_core::util::*;
use backup_core::verify::{verify_backup, Reader, VerifyResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    inv: Mutex<Option<Inventory>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    plan: Mutex<Option<(PathBuf, Vec<RItem>)>>,
}

// ---------------------------------------------------------------- DTO

#[derive(Serialize, Clone)]
struct CatDto {
    id: String,
    title: String,
    desc: String,
    default: bool,
    found: bool,
    note: String,
    size: u64,
    count: u64,
}

#[derive(Serialize, Clone)]
struct ProjDto {
    key: String,
    cwd: String,
    cwd_tilde: String,
    name: String,
    sessions: u64,
    size: u64,
    last: f64,
    last_text: String,
    exists: bool,
    has_files: bool,
}

#[derive(Serialize, Clone)]
struct DestDto {
    label: String,
    path: String,
    path_tilde: String,
    free: u64,
    kind: String,
}

#[derive(Serialize, Clone)]
struct BackupFileDto {
    path: String,
    name: String,
    dir_tilde: String,
    size: u64,
    is_zip: bool,
    mtime: f64,
    when: String,
    kind: String,
}

#[derive(Serialize)]
struct Overview {
    cats: Vec<CatDto>,
    projects: Vec<ProjDto>,
    retention: RetentionInfo,
    schedule: SchedStatus,
    running: Vec<String>,
    home: String,
    dests: Vec<DestDto>,
    cc_version: String,
    convs: u64,
    projects_with_convs: u64,
    claude_size: u64,
    skills: String,
    problems: usize,
    app_path: String,
    app_in_applications: bool,
    last_backup: Option<BackupFileDto>,
    is_mac: bool,
    os: String,
}

#[derive(Serialize, Clone)]
struct JobProgress {
    job: String,
    phase: String,
    frac: f64,
    line1: String,
    line2: String,
}

#[derive(Serialize, Clone)]
struct JobDone {
    job: String,
    ok: bool,
    data: Value,
    error: String,
}

// ---------------------------------------------------------------- pomoćno

#[cfg(unix)]
fn free_space(p: &Path) -> u64 {
    use std::ffi::CString;
    let c = match CString::new(p.to_string_lossy().as_bytes()) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) == 0 {
            (s.f_bavail as u64).saturating_mul(s.f_frsize as u64)
        } else {
            0
        }
    }
}

#[cfg(windows)]
fn free_space(p: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let w: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut avail: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok != 0 {
        avail
    } else {
        0
    }
}

/// Desktop, Documents, Downloads i ~/ClaudeBackups (na Windowsu i kad su preusmereni na OneDrive).
fn user_folders(env: &Env) -> Vec<(&'static str, PathBuf)> {
    #[cfg(windows)]
    {
        let pick = |d: Option<PathBuf>, n: &str| d.unwrap_or_else(|| env.home.join(n));
        vec![
            ("Desktop", pick(dirs::desktop_dir(), "Desktop")),
            ("Documents", pick(dirs::document_dir(), "Documents")),
            ("Downloads", pick(dirs::download_dir(), "Downloads")),
            ("ClaudeBackups", env.home.join("ClaudeBackups")),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![
            ("Desktop", env.home.join("Desktop")),
            ("Documents", env.home.join("Documents")),
            ("Downloads", env.home.join("Downloads")),
            ("ClaudeBackups", env.home.join("ClaudeBackups")),
        ]
    }
}

/// Spoljni diskovi: /Volumes na Mac-u, ostala slova (D:, E:…) na Windowsu.
fn volumes() -> Vec<(String, PathBuf)> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
        let mask = unsafe { GetLogicalDrives() };
        let mut v = vec![];
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            if format!("{letter}:") == sys {
                continue;
            }
            let root = format!("{letter}:\\");
            let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            let t = unsafe { GetDriveTypeW(w.as_ptr()) };
            // 2 = prenosivi (USB), 3 = fiksni disk
            if (t == 2 || t == 3) && Path::new(&root).is_dir() {
                v.push((format!("Disk {letter}:"), PathBuf::from(root)));
            }
        }
        v
    }
    #[cfg(not(windows))]
    {
        use backup_core::env::real;
        let mut v = vec![];
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            let mut vols: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            vols.sort();
            for p in vols {
                if real(&p).map(|r| r.parent().is_some()).unwrap_or(false) && p.is_dir() {
                    v.push((p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), p));
                }
            }
        }
        v
    }
}

fn mtime_secs(p: &Path) -> f64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn dests(env: &Env) -> Vec<DestDto> {
    let mut v = vec![];
    for (label, p) in user_folders(env) {
        if p.is_dir() {
            v.push(DestDto { label: label.into(), path: p.to_string_lossy().into(), path_tilde: env.tilde(&p), free: free_space(&p), kind: "home".into() });
        }
    }
    for (label, p) in volumes() {
        v.push(DestDto { label, path: p.to_string_lossy().into(), path_tilde: p.to_string_lossy().into(), free: free_space(&p), kind: "volume".into() });
    }
    v
}

fn find_backups(env: &Env, inv: Option<&Inventory>) -> Vec<BackupFileDto> {
    let mut locs: Vec<PathBuf> = user_folders(env).into_iter().map(|(_, p)| p).collect();
    let st = schedule_status(env);
    if !st.dest.is_empty() {
        locs.push(PathBuf::from(&st.dest));
    }
    for (_, p) in volumes() {
        locs.push(p.clone());
        locs.push(p.join("ClaudeBackups"));
    }
    if let Some(inv) = inv {
        for p in inv.projects.values() {
            if !p.cwd.is_empty() && p.sessions > 0 {
                for n in backup_core::i18n::PROJECT_DIRS {
                    locs.push(Path::new(&p.cwd).join(n));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for loc in locs {
        if !loc.is_dir() || !seen.insert(loc.clone()) {
            continue;
        }
        let rd = match std::fs::read_dir(&loc) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("ClaudeBackup_") || name.ends_with(".part") {
                continue;
            }
            let is_zip = name.ends_with(".zip") && p.is_file();
            let is_dir = p.is_dir() && p.join("MANIFEST.json").is_file();
            if !is_zip && !is_dir {
                continue;
            }
            let size = if is_zip { std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) } else { 0 };
            let mt = mtime_secs(&p);
            // vrsta kao kod; interfejs je prevodi
            let kind = if backup_core::i18n::PROJECT_DIRS.iter().any(|n| loc.ends_with(n)) {
                "project"
            } else if !st.dest.is_empty() && loc == Path::new(&st.dest) {
                "auto"
            } else {
                "manual"
            };
            out.push(BackupFileDto {
                path: p.to_string_lossy().into(),
                name,
                dir_tilde: env.tilde(&loc),
                size,
                is_zip,
                mtime: mt,
                when: fmt_epoch(mt, true),
                kind: kind.into(),
            });
        }
    }
    out.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn proj_dto(env: &Env, p: &Proj, inv: &Inventory) -> ProjDto {
    let cwd = Path::new(&p.cwd);
    ProjDto {
        key: p.key.clone(),
        cwd: p.cwd.clone(),
        cwd_tilde: if p.cwd.is_empty() { p.key.clone() } else { env.tilde(cwd) },
        name: cwd.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.key.clone()),
        sessions: p.sessions,
        size: p.size,
        last: p.last,
        last_text: fmt_epoch(p.last, false),
        exists: !p.cwd.is_empty() && cwd.is_dir(),
        has_files: inv.cat("project_files").files.iter().any(|f| f.proj == p.key),
    }
}

fn spawn_job<F>(app: AppHandle, busy: Arc<AtomicBool>, job: &str, f: F) -> Result<(), String>
where
    F: FnOnce(&(dyn Fn(Progress) + Sync)) -> Result<Value, String> + Send + 'static,
{
    if busy.swap(true, Ordering::SeqCst) {
        return Err(t("Već radim drugi posao — sačekaj da se završi.", "Already busy with another job — wait for it to finish.").into());
    }
    let job = job.to_string();
    std::thread::spawn(move || {
        let a2 = app.clone();
        let j2 = job.clone();
        let cb = move |p: Progress| {
            let _ = a2.emit("job-progress", JobProgress { job: j2.clone(), phase: p.phase, frac: p.frac, line1: p.line1, line2: p.line2 });
        };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&cb)));
        let done = match r {
            Ok(Ok(data)) => JobDone { job: job.clone(), ok: true, data, error: String::new() },
            Ok(Err(e)) => JobDone { job: job.clone(), ok: false, data: Value::Null, error: e },
            Err(_) => JobDone { job: job.clone(), ok: false, data: Value::Null, error: t("neočekivana greška u programu", "unexpected error in the program").into() },
        };
        busy.store(false, Ordering::SeqCst);
        let _ = app.emit("job-done", done);
    });
    Ok(())
}

fn current_inv(state: &AppState) -> Inventory {
    if let Some(inv) = state.inv.lock().unwrap().clone() {
        return inv;
    }
    let inv = scan(&Env::new());
    *state.inv.lock().unwrap() = Some(inv.clone());
    inv
}

// ---------------------------------------------------------------- komande

#[tauri::command]
async fn overview(state: State<'_, AppState>) -> Result<Overview, String> {
    let inv = tauri::async_runtime::spawn_blocking(|| scan(&Env::new())).await.map_err(|e| e.to_string())?;
    *state.inv.lock().unwrap() = Some(inv.clone());
    let env = inv.env.clone();
    let cats: Vec<CatDto> = inv
        .cats
        .iter()
        .map(|c| CatDto {
            id: c.id.clone(),
            title: c.title.clone(),
            desc: c.desc.clone(),
            default: c.default && !c.files.is_empty(),
            found: !c.files.is_empty(),
            note: cat_note(&inv, &c.id, None),
            size: c.size(),
            count: c.files.len() as u64,
        })
        .collect();
    let mut projects: Vec<ProjDto> = inv.projects.values().filter(|p| p.sessions > 0).map(|p| proj_dto(&env, p, &inv)).collect();
    projects.sort_by(|a, b| b.last.partial_cmp(&a.last).unwrap_or(std::cmp::Ordering::Equal));
    let convs = inv.cat("code_sessions").files.iter().filter(|f| f.is_session_file()).count() as u64;
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let backups = find_backups(&env, Some(&inv));
    let ext_note = cat_note(&inv, "extensions", None);
    Ok(Overview {
        retention: retention_info(&env, &inv.cc_version),
        schedule: schedule_status(&env),
        running: inv.running.clone(),
        home: env.home.to_string_lossy().into(),
        dests: dests(&env),
        cc_version: inv.cc_version.clone(),
        convs,
        projects_with_convs: inv.projects.values().filter(|p| p.sessions > 0).count() as u64,
        claude_size: inv.cats.iter().map(|c| c.size()).sum(),
        skills: ext_note.rsplit_once(" · ").map(|(a, _)| a.to_string()).unwrap_or(ext_note),
        problems: inv.problems.len(),
        app_in_applications: !cfg!(target_os = "macos") || exe.starts_with("/Applications/") || exe.contains("/Applications/"),
        app_path: exe,
        last_backup: backups.into_iter().next(),
        is_mac: cfg!(target_os = "macos"),
        os: if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" }.into(),
        cats,
        projects,
    })
}

#[derive(serde::Deserialize)]
struct BackupReq {
    cats: Vec<String>,
    projects: Option<Vec<String>>,
    html: bool,
    md: bool,
    dest: String,
    zip: bool,
}

#[derive(Serialize)]
struct Estimate {
    files: u64,
    bytes: u64,
    convs: u64,
}

#[tauri::command]
fn estimate(state: State<'_, AppState>, cats: Vec<String>, projects: Option<Vec<String>>) -> Estimate {
    let inv = current_inv(&state);
    let ps: Option<HashSet<String>> = projects.map(|v| v.into_iter().collect());
    let e = select_entries(&inv, &cats, ps.as_ref());
    Estimate {
        files: e.len() as u64,
        bytes: e.iter().map(|f| f.size).sum(),
        convs: e.iter().filter(|f| f.is_session_file()).count() as u64,
    }
}

#[tauri::command]
fn check_dest(path: String) -> Option<String> {
    let env = Env::new();
    dest_ok(&env, Path::new(&expand(&path, &env.home)))
}

#[tauri::command]
fn start_backup(app: AppHandle, state: State<'_, AppState>, req: BackupReq) -> Result<(), String> {
    let env = Env::new();
    let dest = PathBuf::from(expand(&req.dest, &env.home));
    if let Some(e) = dest_ok(&env, &dest) {
        return Err(e);
    }
    let inv = current_inv(&state);
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    spawn_job(app, state.busy.clone(), "backup", move |cb| {
        let mut o = BackupOpts::full(req.cats, dest);
        o.projects = req.projects.map(|v| v.into_iter().collect());
        o.html = req.html;
        o.md = req.md;
        o.as_zip = req.zip;
        let r: BackupResult = run_backup(&inv, &o, cb, Some(cancel.as_ref())).map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted { t("Prekinuto — nedovršeni backup je uklonjen. U Claude-u ništa nije promenjeno.", "Cancelled — the unfinished backup was removed. Nothing in Claude was changed.").into() } else { os_err(&e) }
        })?;
        Ok(json!({"result": r, "lines": backup_core::backup::report_lines(&r)}))
    })
}

#[derive(Serialize)]
struct ProjectPreview {
    key: String,
    name: String,
    cwd_tilde: String,
    dest_tilde: String,
    sessions: u64,
    sessions_size: u64,
    memory: u64,
    images: u64,
    per_session: u64,
    cards: u64,
    project_files: u64,
    prompts: u64,
}

#[tauri::command]
fn project_for_folder(state: State<'_, AppState>, path: String) -> Option<String> {
    let inv = current_inv(&state);
    project_for_path(&inv, Path::new(&path)).map(|p| p.key.clone())
}

#[tauri::command]
fn project_preview(state: State<'_, AppState>, key: String) -> Result<ProjectPreview, String> {
    let inv = current_inv(&state);
    let env = inv.env.clone();
    let p = inv.projects.get(&key).ok_or(t("nepoznat projekat", "unknown project"))?;
    let e = project_entries(&inv, &key);
    let cnt = |cat: &str| e.iter().filter(|f| f.cat == cat).count() as u64;
    Ok(ProjectPreview {
        key: key.clone(),
        name: Path::new(&p.cwd).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        cwd_tilde: env.tilde(Path::new(&p.cwd)),
        dest_tilde: env.tilde(&project_dest(Path::new(&p.cwd))),
        sessions: p.sessions,
        sessions_size: e.iter().filter(|f| f.cat == "code_sessions").map(|f| f.size).sum(),
        memory: e.iter().filter(|f| f.cat == "code_sessions" && f.arc.contains("/memory/")).count() as u64,
        images: cnt("tmp_sessions"),
        per_session: cnt("global"),
        cards: cnt("desktop_code"),
        project_files: cnt("project_files"),
        prompts: project_prompt_history(&env, &p.cwd).iter().filter(|b| **b == b'\n').count() as u64,
    })
}

#[tauri::command]
fn start_project_backup(app: AppHandle, state: State<'_, AppState>, key: String, html: bool, md: bool, zip: bool) -> Result<(), String> {
    let inv = current_inv(&state);
    let p = inv.projects.get(&key).cloned().ok_or(t("nepoznat projekat", "unknown project"))?;
    let dest = project_dest(Path::new(&p.cwd));
    prepare_project_dest(&dest).map_err(|e| tf!("Ne mogu da pišem u folder projekta: {}", "Can't write to the project folder: {}", e))?;
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    spawn_job(app, state.busy.clone(), "backup", move |cb| {
        let env = inv.env.clone();
        let prompts = project_prompt_history(&env, &p.cwd);
        let name = Path::new(&p.cwd).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let o = BackupOpts {
            cats: vec![],
            projects: Some(HashSet::from([p.key.clone()])),
            html,
            md,
            dest,
            as_zip: zip,
            prefix: format!("ClaudeBackup_{}", slug(&name, 30)),
            project_key: Some(p.key.clone()),
            scope: json!({"type": "project", "cwd": p.cwd, "key": p.key}),
            extra: if prompts.is_empty() { vec![] } else { vec![(backup_core::i18n::names().prompts.into(), prompts)] },
        };
        let r = run_backup(&inv, &o, cb, Some(cancel.as_ref())).map_err(|e| os_err(&e))?;
        Ok(json!({"result": r, "lines": backup_core::backup::report_lines(&r)}))
    })
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn list_backups(state: State<'_, AppState>) -> Vec<BackupFileDto> {
    let inv = state.inv.lock().unwrap().clone();
    find_backups(&Env::new(), inv.as_ref())
}

#[tauri::command]
fn start_verify(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    spawn_job(app, state.busy.clone(), "verify", move |cb| {
        let vr: VerifyResult = verify_backup(Path::new(&path), cb);
        Ok(serde_json::to_value(vr).unwrap())
    })
}

#[tauri::command]
fn start_restore_plan(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let app2 = app.clone();
    spawn_job(app, state.busy.clone(), "plan", move |cb| {
        let env = Env::new();
        let p = PathBuf::from(&path);
        let vr = verify_backup(&p, cb);
        if !vr.passed() {
            return Ok(json!({"verify": vr, "cats": Vec::<CatPlan>::new()}));
        }
        let items = plan_restore(&env, vr.manifest.as_ref().unwrap(), cb);
        let cats = summarize(&items);
        let st: State<AppState> = app2.state();
        *st.plan.lock().unwrap() = Some((p, items));
        Ok(json!({"verify": vr, "cats": cats, "retention": retention_info(&env, "")}))
    })
}

#[tauri::command]
fn start_restore(app: AppHandle, state: State<'_, AppState>, cats: Vec<String>, set_ret: bool) -> Result<(), String> {
    let plan = state.plan.lock().unwrap().take().ok_or(t("Prvo izaberi backup.", "Choose a backup first."))?;
    spawn_job(app, state.busy.clone(), "restore", move |cb| {
        let env = Env::new();
        let (path, mut items) = plan;
        let mut rd = Reader::open(&path)?;
        let r: RestoreResult = apply_restore(&mut rd, &mut items, Some(&cats), cb);
        // tek POSLE vraćanja: cleanupPeriodDays se dopisuje u tvoj vraćeni settings.json
        let ret: Option<SetResult> = if set_ret { Some(core_set_retention(&env, backup_core::retention::RECOMMENDED_DAYS)) } else { None };
        Ok(json!({"result": r, "retention": ret}))
    })
}

#[tauri::command]
fn get_retention(state: State<'_, AppState>) -> RetentionInfo {
    let v = state.inv.lock().unwrap().as_ref().map(|i| i.cc_version.clone()).unwrap_or_default();
    retention_info(&Env::new(), &v)
}

#[tauri::command]
fn set_retention(days: i64) -> SetResult {
    core_set_retention(&Env::new(), days)
}

#[tauri::command]
fn schedule_get() -> SchedStatus {
    schedule_status(&Env::new())
}

#[tauri::command]
fn schedule_set(freq: String, hour: u32, minute: u32, dest: String, keep: u32) -> Result<SchedStatus, String> {
    let env = Env::new();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dest = PathBuf::from(expand(&dest, &env.home));
    install_schedule(&env, &exe, &freq, hour, minute, &dest, keep, true)?;
    Ok(schedule_status(&env))
}

#[tauri::command]
fn schedule_remove() -> Result<String, String> {
    remove_schedule(&Env::new())
}

#[tauri::command]
fn schedule_run_now() -> Result<(), String> {
    run_schedule_now(&Env::new())
}

#[tauri::command]
fn reveal(path: String) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}

#[tauri::command]
fn open_path(path: String) {
    let opener = if cfg!(windows) { "explorer.exe" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(path).spawn();
}

#[tauri::command]
fn open_fda_settings() {
    if cfg!(target_os = "macos") {
        let _ = Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
    }
}

#[tauri::command]
fn get_lang() -> String {
    backup_core::i18n::lang().into()
}

/// Menja jezik i pamti ga (važi i za zakazani backup i komandnu liniju).
#[tauri::command]
fn set_lang(state: State<'_, AppState>, lang: String) -> Result<String, String> {
    backup_core::i18n::save_lang(&Env::new(), &lang).map_err(|e| os_err(&e))?;
    // nazivi kategorija dolaze iz skeniranja — sledeći pregled ih pravi na novom jeziku
    *state.inv.lock().unwrap() = None;
    Ok(backup_core::i18n::lang().into())
}

#[tauri::command]
fn app_version() -> String {
    format!("{} · {} {}", env!("CARGO_PKG_VERSION"), t("jezgro", "core"), backup_core::TOOL_VERSION)
}

pub fn run() {
    backup_core::i18n::init(&Env::new());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            overview,
            estimate,
            check_dest,
            start_backup,
            project_for_folder,
            project_preview,
            start_project_backup,
            cancel_job,
            list_backups,
            start_verify,
            start_restore_plan,
            start_restore,
            get_retention,
            set_retention,
            schedule_get,
            schedule_set,
            schedule_remove,
            schedule_run_now,
            reveal,
            open_path,
            open_fda_settings,
            get_lang,
            set_lang,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the app");
}
