//! Vraćanje iz backupa: samo DODAJE fajlove koji fale. Postojeće nikad ne prepisuje i ništa ne briše
//! (osim fajla koji je upravo sam napravio, ako mu kontrolni zbir ne odgovara).

use crate::env::{is_within, Env};
use crate::util::*;
use crate::verify::{safe_arc, Reader};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct RItem {
    pub e: Value,
    pub target: Option<PathBuf>,
    /// add / same / diff / noproj / unsafe
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CatPlan {
    pub id: String,
    pub title: String,
    pub add: u64,
    pub same: u64,
    pub diff: u64,
    pub noproj: u64,
    pub unsafe_: u64,
    pub add_bytes: u64,
}

/// Odakle je fajl: iz „arc” putanje u backupu kad je to Claude-ov folder (radi i ako je backup sa drugog
/// računara ili sistema), inače iz zapisane izvorne putanje.
fn target_from_arc(env: &Env, arc: &str) -> Option<PathBuf> {
    let rest = arc.splitn(2, '/').nth(1).unwrap_or(""); // bez „ClaudeBackup_…/” ako postoji
    let rest = if arc.starts_with("original/") { arc } else { rest };
    let join = |base: &Path, r: &str| -> PathBuf {
        let mut p = base.to_path_buf();
        for c in r.split('/').filter(|c| !c.is_empty()) {
            p.push(c);
        }
        p
    };
    if let Some(r) = rest.strip_prefix("original/dot-claude/") {
        return Some(join(&env.claude, r));
    }
    if rest == "original/claude.json" {
        return Some(env.claude_json.clone());
    }
    if let Some(r) = rest.strip_prefix("original/claude-app/") {
        return Some(join(&env.app, r));
    }
    if let Some(r) = rest.strip_prefix("original/claude-tmp/") {
        return Some(join(&env.tmp, r));
    }
    None
}

fn bad_token(tok: &str) -> bool {
    tok.is_empty() || tok.split(['/', '\\']).any(|p| p == "..")
}

pub fn restore_target(env: &Env, e: &Value) -> (Option<PathBuf>, &'static str) {
    let tok = e["src"].as_str().unwrap_or("");
    if bad_token(tok) {
        return (None, "unsafe");
    }
    if e["cat"].as_str() == Some("project_files") {
        let t = env.from_token(tok);
        let root_tok = e["proj_root"].as_str().unwrap_or("");
        if bad_token(root_tok) {
            return (None, "unsafe");
        }
        let root = env.from_token(root_tok);
        let rel = match t.strip_prefix(&root) {
            Ok(r) => crate::env::slash(r),
            Err(_) => return (None, "unsafe"),
        };
        if !(rel == "CLAUDE.md" || rel == "CLAUDE.local.md" || rel == ".mcp.json" || rel.starts_with(".claude/")) {
            return (None, "unsafe");
        }
        if !root.is_dir() {
            return (Some(t), "noproj");
        }
        return (Some(t), "ok");
    }
    let arc = e["arc"].as_str().unwrap_or("");
    let t = target_from_arc(env, arc).unwrap_or_else(|| env.from_token(tok));
    if ![&env.home, &env.tmp, &env.claude, &env.app].iter().any(|a| is_within(&t, a) && t != **a) {
        return (None, "unsafe");
    }
    (Some(t), "ok")
}

pub fn plan_restore(env: &Env, man: &Value, cb: Cb) -> Vec<RItem> {
    let rep = Reporter::new(cb);
    let files = man["files"].as_array().cloned().unwrap_or_default();
    let n = files.len().max(1);
    let mut items = vec![];
    for (i, e) in files.into_iter().enumerate() {
        rep.emit("plan", i as f64 / n as f64, format!("{} / {}", num(i as u64 + 1), num(n as u64)), e["src"].as_str().unwrap_or(""), false);
        if !safe_arc(e["arc"].as_str().unwrap_or("")) {
            items.push(RItem { e, target: None, status: "unsafe".into() });
            continue;
        }
        let (t, st) = restore_target(env, &e);
        if st != "ok" {
            items.push(RItem { e, target: t, status: st.into() });
            continue;
        }
        let t = t.unwrap();
        if fs::symlink_metadata(&t).is_err() {
            items.push(RItem { e, target: Some(t), status: "add".into() });
            continue;
        }
        let same = fs::metadata(&t).map(|m| m.is_file() && Some(m.len()) == e["size"].as_u64()).unwrap_or(false)
            && sha_file(&t).ok().as_deref() == e["sha256"].as_str();
        items.push(RItem { e, target: Some(t), status: if same { "same" } else { "diff" }.into() });
    }
    items
}

pub fn summarize(items: &[RItem]) -> Vec<CatPlan> {
    let mut by: BTreeMap<String, CatPlan> = BTreeMap::new();
    for it in items {
        let cid = it.e["cat"].as_str().unwrap_or("?").to_string();
        let p = by.entry(cid.clone()).or_insert_with(|| CatPlan {
            id: cid.clone(),
            title: crate::scan::cat_title(&cid).into(),
            ..Default::default()
        });
        match it.status.as_str() {
            "add" => {
                p.add += 1;
                p.add_bytes += it.e["size"].as_u64().unwrap_or(0);
            }
            "same" => p.same += 1,
            "diff" => p.diff += 1,
            "noproj" => p.noproj += 1,
            _ => p.unsafe_ += 1,
        }
    }
    let order: Vec<&str> = crate::scan::CAT_DEFS.iter().map(|c| c.id).collect();
    let mut v: Vec<CatPlan> = by.into_values().collect();
    v.sort_by_key(|p| order.iter().position(|x| *x == p.id).unwrap_or(99));
    v
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RestoreResult {
    pub added: u64,
    pub appeared: u64,
    pub failed: Vec<(String, String)>,
}

pub fn apply_restore(rd: &mut Reader, items: &mut [RItem], cats: Option<&[String]>, cb: Cb) -> RestoreResult {
    let rep = Reporter::new(cb);
    let mut res = RestoreResult::default();
    let todo: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.status == "add" && cats.map(|c| c.iter().any(|x| Some(x.as_str()) == it.e["cat"].as_str())).unwrap_or(true))
        .map(|(i, _)| i)
        .collect();
    let n = todo.len().max(1);
    for (k, &i) in todo.iter().enumerate() {
        let it = &mut items[i];
        let e = it.e.clone();
        let t = it.target.clone().unwrap();
        rep.emit("restore", k as f64 / n as f64, format!("{} / {}", num(k as u64 + 1), num(todo.len() as u64)), e["src"].as_str().unwrap_or(""), false);
        let r = (|| -> io::Result<bool> {
            if let Some(p) = t.parent() {
                fs::create_dir_all(p)?;
            }
            // create_new = nikad ne prepisuje postojeći fajl
            let mut dst = match OpenOptions::new().write(true).create_new(true).open(&t) {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
                Err(e) => return Err(e),
            };
            let mut hash_ok = false;
            let arc = e["arc"].as_str().unwrap_or("").to_string();
            let copy = rd.with(&arc, &mut |r: &mut dyn Read| {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                let mut buf = vec![0u8; 1 << 20];
                loop {
                    let k = r.read(&mut buf)?;
                    if k == 0 {
                        break;
                    }
                    h.update(&buf[..k]);
                    dst.write_all(&buf[..k])?;
                }
                hash_ok = Some(hex::encode(h.finalize()).as_str()) == e["sha256"].as_str();
                Ok(())
            });
            if copy.is_err() || !hash_ok {
                drop(dst);
                let _ = fs::remove_file(&t); // samo fajl koji smo upravo mi napravili
                return Err(copy.err().unwrap_or_else(|| io::Error::other(crate::i18n::t("kontrolni zbir se ne slaže", "checksum doesn't match"))));
            }
            let mt = e["mtime_ns"].as_i64().unwrap_or(0).max(0) as u64;
            let when = UNIX_EPOCH + Duration::from_nanos(mt);
            let _ = dst.set_times(fs::FileTimes::new().set_modified(when).set_accessed(when));
            set_mode(&dst, e["mode"].as_u64().unwrap_or(0o600) as u32 & 0o777);
            Ok(true)
        })();
        match r {
            Ok(true) => res.added += 1,
            Ok(false) => {
                it.status = "diff".into();
                res.appeared += 1;
            }
            Err(err) => res.failed.push((e["src"].as_str().unwrap_or("").to_string(), os_err(&err))),
        }
    }
    res
}

pub fn target_display(env: &Env, p: &Path) -> String {
    env.tilde(p)
}
