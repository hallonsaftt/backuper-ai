//! Sve putanje koje program koristi — za macOS, Windows i Linux.
//! HOME (na Windowsu USERPROFILE/APPDATA/TEMP) se poštuje, pa može da se testira na lažnom home folderu.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Env {
    pub home: PathBuf,
    pub claude: PathBuf,
    pub claude_json: PathBuf,
    /// Podaci Claude desktop aplikacije (Code tab, Cowork, MCP podešavanja)
    pub app: PathBuf,
    /// Gde Claude Code drži slike i scratchpad
    pub tmp: PathBuf,
    pub managed: PathBuf,
    /// Podaci ovog programa (last_run.json, zakazivanje na Windowsu) — na Mac-u zajedničko sa Python verzijom
    pub support: PathBuf,
}

pub fn uid() -> u32 {
    #[cfg(unix)]
    unsafe {
        libc::getuid()
    }
    #[cfg(not(unix))]
    0
}

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn find_tmp_root() -> PathBuf {
    if cfg!(windows) {
        // %TEMP%\claude\<projekat>\<sesija>\…  (CLAUDE_CODE_TMPDIR ga pomera)
        let base = var("CLAUDE_CODE_TMPDIR").unwrap_or_else(std::env::temp_dir);
        return base.join("claude");
    }
    let uid = uid();
    if let Some(env) = var("CLAUDE_CODE_TMPDIR") {
        let a = env.join(format!("claude-{uid}"));
        let b = env.join("claude");
        return if !a.is_dir() && b.is_dir() { b } else { a };
    }
    if cfg!(target_os = "macos") {
        return PathBuf::from(format!("/private/tmp/claude-{uid}"));
    }
    for base in [var("TMPDIR"), Some(PathBuf::from("/tmp"))].into_iter().flatten() {
        let c = base.join(format!("claude-{uid}"));
        if c.is_dir() {
            return c;
        }
    }
    PathBuf::from(format!("/tmp/claude-{uid}"))
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH)
}

/// Windows: klasični instaler koristi %APPDATA%\Claude, a Microsoft Store/MSIX verzija
/// %LOCALAPPDATA%\Packages\Claude_…\LocalCache\Roaming\Claude. Ako postoje oba, uzima onaj koji se skorije koristio.
fn windows_app_dir(home: &Path) -> PathBuf {
    let roaming = var("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming")).join("Claude");
    let local = var("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local"));
    let mut cands = vec![roaming.clone()];
    if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
        for e in rd.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("Claude_") {
                cands.push(e.path().join("LocalCache").join("Roaming").join("Claude"));
            }
        }
    }
    let used = |d: &PathBuf| -> std::time::SystemTime {
        ["claude-code-sessions", "local-agent-mode-sessions", "config.json", "claude_desktop_config.json"]
            .iter()
            .map(|n| mtime(&d.join(n)))
            .max()
            .unwrap_or(std::time::UNIX_EPOCH)
    };
    cands.into_iter().filter(|d| d.is_dir()).max_by_key(used).unwrap_or(roaming)
}

impl Env {
    pub fn new() -> Env {
        let home = if cfg!(windows) {
            var("USERPROFILE").or_else(|| var("HOME")).unwrap_or_else(|| PathBuf::from("C:\\"))
        } else {
            var("HOME").unwrap_or_else(|| PathBuf::from("/"))
        };
        let cfg = var("CLAUDE_CONFIG_DIR");
        let claude = match &cfg {
            Some(c) => PathBuf::from(expand(&c.to_string_lossy(), &home)),
            None => home.join(".claude"),
        };
        let claude_json = if cfg.is_some() && claude.join(".claude.json").exists() {
            claude.join(".claude.json")
        } else {
            home.join(".claude.json")
        };
        let (app, managed, support) = if cfg!(windows) {
            let pf = var("ProgramFiles").unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
            let roaming = var("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"));
            (windows_app_dir(&home), pf.join("ClaudeCode").join("managed-settings.json"), roaming.join("ClaudeBackupTool"))
        } else {
            // Linux koristi isti raspored kao Mac (tako rade i Python verzija i testovi)
            let managed = if cfg!(target_os = "macos") {
                PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json")
            } else {
                PathBuf::from("/etc/claude-code/managed-settings.json")
            };
            (home.join("Library/Application Support/Claude"), managed, home.join("Library/Application Support/ClaudeBackupTool"))
        };
        Env { tmp: find_tmp_root(), home, claude, claude_json, app, managed, support }
    }

    /// Putanja zapisana tako da radi i na drugom računaru: ~ i $TMP umesto korisničkog foldera, uvek sa „/”.
    pub fn token(&self, p: &Path) -> String {
        for (base, tok) in [(&self.tmp, "$TMP"), (&self.home, "~")] {
            if let Ok(rel) = p.strip_prefix(base) {
                let r = slash(rel);
                return if r.is_empty() { tok.into() } else { format!("{tok}/{r}") };
            }
        }
        slash(p)
    }

    pub fn from_token(&self, tok: &str) -> PathBuf {
        let (base, rest) = if tok == "$TMP" || tok.starts_with("$TMP/") {
            (&self.tmp, &tok[4..])
        } else if tok == "~" || tok.starts_with("~/") {
            (&self.home, &tok[1..])
        } else {
            return PathBuf::from(tok);
        };
        let mut p = base.clone();
        for c in rest.split('/').filter(|c| !c.is_empty()) {
            p.push(c);
        }
        p
    }

    /// Za prikaz: na Mac-u/Linuxu ~/…, na Windowsu puna putanja (C:\Users\…).
    pub fn tilde(&self, p: &Path) -> String {
        if cfg!(windows) {
            return p.to_string_lossy().to_string();
        }
        match p.strip_prefix(&self.home) {
            Ok(rel) if rel.as_os_str().is_empty() => "~".into(),
            Ok(rel) => format!("~/{}", slash(rel)),
            Err(_) => p.to_string_lossy().to_string(),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.claude.join("settings.json")
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}

/// Relativna ili apsolutna putanja zapisana sa „/” (na Windowsu menja „\”).
pub fn slash(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s
    }
}

pub fn expand(p: &str, home: &Path) -> String {
    if p == "~" {
        return home.to_string_lossy().into();
    }
    if let Some(rest) = p.strip_prefix("~/").or_else(|| if cfg!(windows) { p.strip_prefix("~\\") } else { None }) {
        let mut h = home.to_path_buf();
        for c in rest.split(['/', '\\']).filter(|c| !c.is_empty()) {
            h.push(c);
        }
        return h.to_string_lossy().into();
    }
    p.into()
}

/// Ima li `p` ispod `base` (ili je isto)? Poredi delove putanje, ne tekst.
pub fn is_within(p: &Path, base: &Path) -> bool {
    p.starts_with(base)
}

/// Prava putanja bez Windows „\\?\” prefiksa.
pub fn real<P: AsRef<Path>>(p: P) -> std::io::Result<PathBuf> {
    dunce::canonicalize(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_at(home: &str, tmp: &str) -> Env {
        let h = PathBuf::from(home);
        Env { claude: h.join(".claude"), claude_json: h.join(".claude.json"), app: h.join("app"), tmp: PathBuf::from(tmp),
              managed: PathBuf::from("/m"), support: h.join("s"), home: h }
    }

    #[test]
    fn token_roundtrip() {
        let e = env_at("/Users/pera", "/private/tmp/claude-501");
        let p = Path::new("/Users/pera/.claude/projects/x/a b.jsonl");
        assert_eq!(e.token(p), "~/.claude/projects/x/a b.jsonl");
        assert_eq!(e.from_token(&e.token(p)), p);
        let t = Path::new("/private/tmp/claude-501/p/s/img.png");
        assert_eq!(e.token(t), "$TMP/p/s/img.png");
        assert_eq!(e.from_token("$TMP/p/s/img.png"), t);
        assert_eq!(e.token(Path::new("/Users/pera")), "~");
        assert_eq!(e.token(Path::new("/Users/peraX/f")), "/Users/peraX/f");
        assert_eq!(e.tilde(Path::new("/Users/pera/Desktop")), if cfg!(windows) { "/Users/pera/Desktop" } else { "~/Desktop" });
    }

    #[test]
    fn within_is_by_components() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a/bc"), Path::new("/a/b")));
    }

    #[test]
    fn expand_home() {
        assert_eq!(PathBuf::from(expand("~/X/Y", Path::new("/h"))), Path::new("/h/X/Y"));
        assert_eq!(expand("/abs", Path::new("/h")), "/abs");
    }
}
