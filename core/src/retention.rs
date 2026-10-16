//! „Čuvanje istorije” — cleanupPeriodDays u ~/.claude/settings.json.
//! Menja samo taj ključ; ostatak fajla ostaje kakav jeste. Pre izmene pravi kopiju, piše atomski i proverava.

use crate::env::Env;
use crate::scan::load_json;
use crate::util::*;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

pub const RECOMMENDED_DAYS: i64 = 3650;
pub const DEFAULT_CLEANUP: i64 = 30;
pub const DESKTOP_KEEP_VERSION: [u32; 3] = [2, 1, 248];

#[derive(Clone, Debug, Serialize)]
pub struct RetentionInfo {
    pub cleanup: Option<i64>,
    pub desktop: Option<i64>,
    pub managed: Option<i64>,
    pub effective: i64,
    pub error: String,
    pub version: String,
    pub desktop_kept: bool,
    pub version_known: bool,
}

pub fn read_settings(env: &Env) -> Result<(Map<String, Value>, String), String> {
    let p = env.settings_path();
    if !p.exists() {
        return Ok((Map::new(), String::new()));
    }
    let raw = fs::read_to_string(&p).map_err(|e| format!("ne mogu da pročitam {} ({})", env.tilde(&p), os_err(&e)))?;
    if raw.trim().is_empty() {
        return Ok((Map::new(), raw));
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(o)) => Ok((o, raw)),
        Ok(_) => Err(format!("{} nema očekivani oblik — ne diram ga", env.tilde(&p))),
        Err(e) => Err(format!("{} nije ispravan JSON ({}) — ne diram ga", env.tilde(&p), e)),
    }
}

pub fn retention_info(env: &Env, version: &str) -> RetentionInfo {
    let (d, err) = match read_settings(env) {
        Ok((d, _)) => (Some(d), String::new()),
        Err(e) => (None, e),
    };
    let geti = |k: &str| d.as_ref().and_then(|d| d.get(k)).and_then(|v| v.as_i64());
    let cleanup = geti("cleanupPeriodDays");
    let desktop = geti("desktopSessionCleanupPeriodDays");
    let managed = load_json(&env.managed).and_then(|m| m.get("cleanupPeriodDays").and_then(|v| v.as_i64()));
    let effective = managed.or(cleanup).unwrap_or(DEFAULT_CLEANUP);
    let vt = vtuple(version);
    let version_known = !version.is_empty();
    let desktop_kept = version_known && vt.as_slice() >= &DESKTOP_KEEP_VERSION[..] && desktop.is_none() && managed.is_none();
    RetentionInfo { cleanup, desktop, managed, effective, error: err, version: version.into(), desktop_kept, version_known }
}

fn patch_json_text(raw: &str, key: &str, value: i64, before: &Map<String, Value>) -> String {
    let mut expected = before.clone();
    expected.insert(key.into(), Value::from(value));
    let exp_v = Value::Object(expected.clone());
    if !raw.trim().is_empty() {
        let new = if before.contains_key(key) {
            let pat = Regex::new(&format!(r#"("{}"\s*:\s*)(-?\d+(?:\.\d+)?|null|true|false|"[^"]*")"#, regex::escape(key))).unwrap();
            pat.replacen(raw, 1, |c: &regex::Captures| format!("{}{}", &c[1], value)).to_string()
        } else if let Some(i) = raw.find('{') {
            if raw[i + 1..].trim_start().starts_with('}') {
                let j = i + 1 + raw[i + 1..].find('}').unwrap();
                format!("{}{{\n  \"{}\": {}\n}}{}", &raw[..i], key, value, &raw[j + 1..])
            } else if !raw[i + 1..].trim_start_matches([' ', '\t']).starts_with(['\n', '\r']) {
                // sve u jednom redu → ubaci u isti red
                format!("{}\"{}\": {}, {}", &raw[..i + 1], key, value, raw[i + 1..].trim_start())
            } else {
                // uvuci isto kao prvi ključ u fajlu
                let indent: String = raw[i + 1..]
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
                    .filter(|s: &String| !s.is_empty())
                    .unwrap_or_else(|| "  ".into());
                format!("{}\n{}\"{}\": {},{}", &raw[..i + 1], indent, key, value, &raw[i + 1..])
            }
        } else {
            String::new()
        };
        if !new.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(&new) {
                if v == exp_v {
                    return if new.ends_with('\n') { new } else { new + "\n" };
                }
            }
        }
    }
    serde_json::to_string_pretty(&exp_v).unwrap() + "\n"
}

#[derive(Clone, Debug, Serialize)]
pub struct SetResult {
    pub ok: bool,
    pub message: String,
    pub backup: String,
    pub changes: Vec<String>,
}

pub fn set_retention(env: &Env, days: i64) -> SetResult {
    let fail = |m: String| SetResult { ok: false, message: m, backup: String::new(), changes: vec![] };
    if days < 1 {
        return fail("broj dana mora biti 1 ili više".into());
    }
    let (d, raw) = match read_settings(env) {
        Ok(x) => x,
        Err(e) => return fail(e),
    };
    let p = env.settings_path();
    let mut data = d.clone();
    let old = d.get("cleanupPeriodDays").and_then(|v| v.as_i64());
    let mut new_raw = patch_json_text(&raw, "cleanupPeriodDays", days, &data);
    data.insert("cleanupPeriodDays".into(), Value::from(days));
    let mut changes = vec![format!(
        "\"cleanupPeriodDays\": {} → {}",
        old.map(|o| o.to_string()).unwrap_or_else(|| "(nije podešeno = 30)".into()),
        days
    )];
    if let Some(desk) = d.get("desktopSessionCleanupPeriodDays").and_then(|v| v.as_i64()) {
        if desk < days {
            new_raw = patch_json_text(&new_raw, "desktopSessionCleanupPeriodDays", days, &data);
            data.insert("desktopSessionCleanupPeriodDays".into(), Value::from(days));
            changes.push(format!("\"desktopSessionCleanupPeriodDays\": {desk} → {days}"));
        }
    }
    if old == Some(days) && changes.len() == 1 {
        return SetResult { ok: true, message: "već je tako podešeno — ništa nije menjano".into(), backup: String::new(), changes: vec![] };
    }
    let mut backup = String::new();
    let r = (|| -> std::io::Result<Value> {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        if p.exists() {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let mut b = p.with_file_name(format!("settings.json.pre-claude-backup-{stamp}"));
            let mut n = 2;
            while b.exists() {
                b = p.with_file_name(format!("settings.json.pre-claude-backup-{stamp}-{n}"));
                n += 1;
            }
            fs::copy(&p, &b)?;
            backup = env.tilde(&b);
        }
        let tmp = p.with_file_name(format!(".settings.json.claude-backup-{}.tmp", std::process::id()));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(new_raw.as_bytes())?;
            f.sync_all()?;
        }
        if let Ok(m) = fs::metadata(&p) {
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(m.permissions().mode() & 0o7777));
        }
        fs::rename(&tmp, &p)?;
        let check: Value = serde_json::from_str(&fs::read_to_string(&p)?).map_err(std::io::Error::other)?;
        Ok(check)
    })();
    match r {
        Ok(check) if check == Value::Object(data.clone()) => SetResult { ok: true, message: "sačuvano i provereno".into(), backup, changes },
        Ok(_) => {
            if !backup.is_empty() {
                let _ = fs::copy(env.from_token(&backup), &p);
            }
            SetResult { ok: false, message: "provera posle upisa nije prošla — vraćen je stari fajl".into(), backup, changes: vec![] }
        }
        Err(e) => SetResult { ok: false, message: format!("izmena nije uspela ({}); tvoj fajl nije diran", os_err(&e)), backup, changes: vec![] },
    }
}
