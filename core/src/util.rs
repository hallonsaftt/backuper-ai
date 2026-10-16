//! Sitne pomoćne funkcije: formatiranje brojeva, srpska množina, slug, SHA-256, vreme.

use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Instant;

pub fn human(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v < 1024.0 || unit == "TB" {
            return if unit == "B" {
                format!("{} B", n)
            } else {
                format!("{:.1} {}", v, unit).replace('.', ",")
            };
        }
        v /= 1024.0;
    }
    String::new()
}

pub fn num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Srpska množina: 1 fajl, 2 fajla, 5 fajlova (11–14 → "many").
pub fn plural(n: u64, one: &str, few: &str, many: &str) -> String {
    format!("{} {}", num(n), plural_word(n, one, few, many))
}

pub fn plural_word<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (a, b) = (n % 10, n % 100);
    if a == 1 && b != 11 {
        one
    } else if (2..=4).contains(&a) && !(12..=14).contains(&b) {
        few
    } else {
        many
    }
}

pub fn n_files(n: u64) -> String {
    plural(n, "fajl", "fajla", "fajlova")
}
pub fn n_convs(n: u64) -> String {
    plural(n, "razgovor", "razgovora", "razgovora")
}
pub fn n_projs(n: u64) -> String {
    plural(n, "projekat", "projekta", "projekata")
}

pub fn slug(s: &str, maxlen: usize) -> String {
    let s = s.replace('đ', "dj").replace('Đ', "Dj");
    let a = deunicode::deunicode(&s);
    let mut out = String::new();
    let mut dash = false;
    for c in a.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let t: String = out.trim_matches('-').chars().take(maxlen).collect();
    let t = t.trim_matches('-').to_string();
    if t.is_empty() {
        "bez-naslova".into()
    } else {
        t
    }
}

/// Isto kao Claude Code: svaki znak koji nije slovo/broj (ASCII) postaje '-'.
pub fn proj_key(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

pub fn sha_bytes(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

pub fn sha_file(p: &Path) -> io::Result<String> {
    let mut f = File::open(p)?;
    sha_reader(&mut f).map(|(h, _)| h)
}

pub fn sha_reader<R: Read + ?Sized>(r: &mut R) -> io::Result<(String, u64)> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = r.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((hex::encode(h.finalize()), n))
}

pub fn parse_ts(ts: &str) -> Option<DateTime<Local>> {
    if ts.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(&ts.replace('Z', "+00:00"))
        .ok()
        .map(|d| d.with_timezone(&Local))
}

pub fn fmt_ts(ts: &str, with_time: bool) -> String {
    match parse_ts(ts) {
        Some(d) => d.format(if with_time { "%d.%m.%Y. %H:%M" } else { "%d.%m.%Y." }).to_string(),
        None => ts.chars().take(16).collect::<String>().replace('T', " "),
    }
}

pub fn fmt_epoch(secs: f64, with_time: bool) -> String {
    if secs <= 0.0 {
        return String::new();
    }
    match Local.timestamp_opt(secs as i64, 0).single() {
        Some(d) => d.format(if with_time { "%d.%m.%Y. %H:%M" } else { "%d.%m.%Y." }).to_string(),
        None => String::new(),
    }
}

pub fn now_iso_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

/// Razumljiva poruka za grešku pri čitanju; za macOS zabranu pristupa kaže i kako da se reši.
pub fn os_err(e: &io::Error) -> String {
    let mut msg = match e.raw_os_error() {
        Some(code) => {
            let s = io::Error::from_raw_os_error(code).to_string();
            s.split(" (os error").next().unwrap_or(&s).to_string()
        }
        None => e.to_string(),
    };
    if cfg!(target_os = "macos") && e.kind() == io::ErrorKind::PermissionDenied {
        msg.push_str(
            " — macOS ne dozvoljava pristup; dozvoli u System Settings → Privacy & Security → \
             Full Disk Access za Claude Backup",
        );
    }
    msg
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#x27;"),
            _ => o.push(c),
        }
    }
    o
}

pub fn vtuple(v: &str) -> Vec<u32> {
    v.split('.').take(3).map(|x| x.parse().unwrap_or(0)).collect()
}

// ---------------------------------------------------------------- napredak

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub phase: String,
    pub frac: f64,
    pub line1: String,
    pub line2: String,
}

pub type Cb<'a> = &'a (dyn Fn(Progress) + Sync);

pub fn no_progress() -> impl Fn(Progress) + Sync {
    |_p: Progress| {}
}

/// Javlja napredak najviše ~15 puta u sekundi (osim kad je `force`).
pub struct Reporter<'a> {
    cb: Cb<'a>,
    last: Cell<Option<Instant>>,
}

impl<'a> Reporter<'a> {
    pub fn new(cb: Cb<'a>) -> Self {
        Reporter { cb, last: Cell::new(None) }
    }
    pub fn emit(&self, phase: &str, frac: f64, line1: impl Into<String>, line2: impl Into<String>, force: bool) {
        let now = Instant::now();
        if !force {
            if let Some(t) = self.last.get() {
                if now.duration_since(t).as_millis() < 66 && frac < 1.0 {
                    return;
                }
            }
        }
        self.last.set(Some(now));
        (self.cb)(Progress {
            phase: phase.into(),
            frac: frac.clamp(0.0, 1.0),
            line1: line1.into(),
            line2: line2.into(),
        });
    }
}
