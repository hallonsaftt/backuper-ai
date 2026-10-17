//! Small helpers: number/size/date formatting (English or Serbian), plurals, slug, SHA-256, time.

use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Instant;

/// 1.2 MB (English) / 1,2 MB (srpski)
pub fn human(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v < 1024.0 || unit == "TB" {
            return if unit == "B" {
                format!("{} B", n)
            } else {
                let s = format!("{:.1} {}", v, unit);
                if crate::i18n::sr() { s.replace('.', ",") } else { s }
            };
        }
        v /= 1024.0;
    }
    String::new()
}

/// 12,345 (English) / 12.345 (srpski)
pub fn num(n: u64) -> String {
    let sep = if crate::i18n::sr() { '.' } else { ',' };
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Srpska množina: 1 fajl, 2 fajla, 5 fajlova (11–14 → "many").
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

/// Number + word in the current language: pl(5, ("fajl", "fajla", "fajlova"), ("file", "files")).
pub fn pl(n: u64, sr: (&str, &str, &str), en: (&str, &str)) -> String {
    if crate::i18n::sr() {
        format!("{} {}", num(n), plural_word(n, sr.0, sr.1, sr.2))
    } else {
        format!("{} {}", num(n), if n == 1 { en.0 } else { en.1 })
    }
}

pub fn n_files(n: u64) -> String {
    pl(n, ("fajl", "fajla", "fajlova"), ("file", "files"))
}
pub fn n_convs(n: u64) -> String {
    pl(n, ("razgovor", "razgovora", "razgovora"), ("conversation", "conversations"))
}
pub fn n_projs(n: u64) -> String {
    pl(n, ("projekat", "projekta", "projekata"), ("project", "projects"))
}
pub fn n_warnings(n: u64) -> String {
    pl(n, ("upozorenje", "upozorenja", "upozorenja"), ("warning", "warnings"))
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
        crate::i18n::t("bez-naslova", "untitled").into()
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

fn date_fmt(with_time: bool) -> &'static str {
    match (crate::i18n::sr(), with_time) {
        (true, true) => "%d.%m.%Y. %H:%M",
        (true, false) => "%d.%m.%Y.",
        (false, true) => "%b %-d, %Y %H:%M",
        (false, false) => "%b %-d, %Y",
    }
}

pub fn fmt_ts(ts: &str, with_time: bool) -> String {
    match parse_ts(ts) {
        Some(d) => d.format(date_fmt(with_time)).to_string(),
        None => ts.chars().take(16).collect::<String>().replace('T', " "),
    }
}

pub fn fmt_epoch(secs: f64, with_time: bool) -> String {
    if secs <= 0.0 {
        return String::new();
    }
    match Local.timestamp_opt(secs as i64, 0).single() {
        Some(d) => d.format(date_fmt(with_time)).to_string(),
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
        msg.push_str(crate::i18n::t(
            " — macOS ne dozvoljava pristup; dozvoli u System Settings → Privacy & Security → \
             Full Disk Access za Claude Backup",
            " — macOS blocked access; allow it in System Settings → Privacy & Security → \
             Full Disk Access for Claude Backup",
        ));
    }
    msg
}

// ---------------------------------------------------------------- razlike između sistema

/// Vreme poslednje izmene u nanosekundama od 1970.
pub fn meta_mtime_ns(m: &std::fs::Metadata) -> i64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.mtime() * 1_000_000_000 + m.mtime_nsec()
    }
    #[cfg(not(unix))]
    {
        match m.modified() {
            Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_nanos() as i64,
                Err(e) => -(e.duration().as_nanos() as i64),
            },
            Err(_) => 0,
        }
    }
}

/// Unix dozvole (na Windowsu približno: samo-za-čitanje ili ne).
pub fn meta_mode(m: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.mode()
    }
    #[cfg(not(unix))]
    {
        let base = if m.is_dir() { 0o040755 } else { 0o100644 };
        if m.permissions().readonly() {
            base & !0o222
        } else {
            base
        }
    }
}

/// Postavlja unix dozvole; na Windowsu ne radi ništa.
#[allow(unused_variables)]
pub fn set_mode(f: &std::fs::File, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = f.set_permissions(std::fs::Permissions::from_mode(mode));
    }
}

#[allow(unused_variables)]
pub fn set_path_mode(p: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

/// Spoljni program bez treptanja crnog prozora na Windowsu.
pub fn command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

pub fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if r == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            return String::from_utf8_lossy(&buf[..end]).to_string();
        }
        "?".into()
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "?".into())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_per_language() {
        let _g = crate::i18n::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::i18n::set_lang("sr");
        assert_eq!(num(1284), "1.284");
        assert_eq!(human(537_100_000), "512,2 MB");
        assert_eq!(n_files(1), "1 fajl");
        assert_eq!(n_files(3), "3 fajla");
        assert_eq!(n_files(12), "12 fajlova");
        assert_eq!(n_files(21), "21 fajl");
        crate::i18n::set_lang("en");
        assert_eq!(num(1284), "1,284");
        assert_eq!(human(537_100_000), "512.2 MB");
        assert_eq!(n_files(1), "1 file");
        assert_eq!(n_files(21), "21 files");
        assert_eq!(n_convs(0), "0 conversations");
    }
}
