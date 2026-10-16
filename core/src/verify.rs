//! Čitanje backupa (ZIP ili folder) i provera svakog fajla po SHA-256 iz MANIFEST-a.

use crate::util::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize)]
pub struct VerifyResult {
    pub ok: u64,
    pub total: u64,
    pub bytes: u64,
    pub bad: Vec<String>,
    pub missing: Vec<String>,
    pub error: String,
    pub passed: bool,
    pub created: String,
    pub hostname: String,
    pub claude_code_version: String,
    pub scope: String,
    #[serde(skip)]
    pub manifest: Option<Value>,
}

impl VerifyResult {
    pub fn passed(&self) -> bool {
        self.error.is_empty() && self.bad.is_empty() && self.missing.is_empty() && self.ok == self.total && self.total > 0
    }
}

pub fn safe_arc(arc: &str) -> bool {
    !arc.is_empty() && !arc.starts_with('/') && !arc.split('/').any(|p| p == "..") && !arc.contains('\\')
}

pub enum Reader {
    Zip { zip: zip::ZipArchive<File>, prefix: String, names: HashSet<String> },
    Dir { root: PathBuf },
}

impl Reader {
    pub fn open(path: &Path) -> Result<Reader, String> {
        let md = fs::metadata(path).map_err(|e| os_err(&e))?;
        if md.is_dir() {
            if !path.join("MANIFEST.json").is_file() {
                return Err("u ovom folderu nema MANIFEST.json — nije Claude backup".into());
            }
            return Ok(Reader::Dir { root: path.to_path_buf() });
        }
        let f = File::open(path).map_err(|e| os_err(&e))?;
        let zip = zip::ZipArchive::new(f).map_err(|e| format!("nije ispravan ZIP ({e})"))?;
        let names: HashSet<String> = zip.file_names().map(|s| s.to_string()).collect();
        let mut mans: Vec<&String> = names.iter().filter(|n| n.ends_with("/MANIFEST.json") && n.matches('/').count() == 1).collect();
        mans.sort();
        let prefix = if let Some(m) = mans.first() {
            m[..m.len() - "MANIFEST.json".len()].to_string()
        } else if names.contains("MANIFEST.json") {
            String::new()
        } else {
            return Err("u ZIP-u nema MANIFEST.json — nije Claude backup".into());
        };
        Ok(Reader::Zip { zip, prefix, names })
    }

    pub fn manifest(&mut self) -> Result<Value, String> {
        let mut buf = vec![];
        self.with(&"MANIFEST.json".to_string(), &mut |r| r.read_to_end(&mut buf).map(|_| ()))
            .map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_slice(&buf).map_err(|e| format!("MANIFEST.json nije ispravan ({e})"))?;
        if v.get("format").and_then(|f| f.as_str()) != Some("claude-backup") {
            return Err("MANIFEST.json nije prepoznat".into());
        }
        Ok(v)
    }

    pub fn has(&self, arc: &str) -> bool {
        match self {
            Reader::Zip { prefix, names, .. } => names.contains(&format!("{prefix}{arc}")),
            Reader::Dir { root } => root.join(arc).is_file(),
        }
    }

    /// Otvori član backupa i prosledi ga funkciji `f`.
    pub fn with(&mut self, arc: &str, f: &mut dyn FnMut(&mut dyn Read) -> io::Result<()>) -> io::Result<()> {
        match self {
            Reader::Zip { zip, prefix, .. } => {
                let mut z = zip.by_name(&format!("{prefix}{arc}")).map_err(io::Error::other)?;
                f(&mut z)
            }
            Reader::Dir { root } => {
                let mut file = File::open(root.join(arc))?;
                f(&mut file)
            }
        }
    }
}

pub fn verify_backup(path: &Path, cb: Cb) -> VerifyResult {
    let rep = Reporter::new(cb);
    let mut vr = VerifyResult::default();
    let mut rd = match Reader::open(path) {
        Ok(r) => r,
        Err(e) => {
            vr.error = e;
            return vr;
        }
    };
    let man = match rd.manifest() {
        Ok(m) => m,
        Err(e) => {
            vr.error = e;
            return vr;
        }
    };
    vr.created = man["created"].as_str().unwrap_or("").into();
    vr.hostname = man["hostname"].as_str().unwrap_or("").into();
    vr.claude_code_version = man["claude_code_version"].as_str().unwrap_or("").into();
    vr.scope = man["scope"]["type"].as_str().unwrap_or("full").into();
    let mut entries: Vec<Value> = man["files"].as_array().cloned().unwrap_or_default();
    entries.extend(man["generated"].as_array().cloned().unwrap_or_default());
    vr.total = entries.len() as u64;
    let total: u64 = entries.iter().map(|e| e["size"].as_u64().unwrap_or(0)).sum::<u64>().max(1);
    let mut done = 0u64;
    let n = entries.len();
    for (i, e) in entries.iter().enumerate() {
        let arc = e["arc"].as_str().unwrap_or("").to_string();
        rep.emit("verify", done as f64 / total as f64, format!("{} / {} fajlova", num(i as u64 + 1), num(n as u64)), arc.clone(), i + 1 == n);
        if !safe_arc(&arc) {
            vr.bad.push(arc);
            continue;
        }
        if !rd.has(&arc) {
            vr.missing.push(arc);
            continue;
        }
        let mut res: Option<(String, u64)> = None;
        let r = rd.with(&arc, &mut |r| {
            res = Some(sha_reader(r)?);
            Ok(())
        });
        match (r, res) {
            (Ok(()), Some((h, k))) if Some(h.as_str()) == e["sha256"].as_str() && Some(k) == e["size"].as_u64() => {
                vr.ok += 1;
                vr.bytes += k;
            }
            _ => vr.bad.push(arc),
        }
        done += e["size"].as_u64().unwrap_or(0);
    }
    vr.manifest = Some(man);
    vr.passed = vr.passed();
    vr
}
