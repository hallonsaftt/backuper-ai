//! Sve putanje koje program koristi. HOME se poštuje, pa može da se testira na lažnom HOME-u.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Env {
    pub home: PathBuf,
    pub claude: PathBuf,
    pub claude_json: PathBuf,
    pub app: PathBuf,
    pub tmp: PathBuf,
    pub managed: PathBuf,
    /// ~/Library/Application Support/ClaudeBackupTool — zajedničko sa Python verzijom (last_run.json…)
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

fn find_tmp_root() -> PathBuf {
    // Gde Claude Code drži slike i scratchpad (isto kao Python verzija).
    let uid = uid();
    if let Ok(env) = std::env::var("CLAUDE_CODE_TMPDIR") {
        if !env.is_empty() {
            let a = Path::new(&env).join(format!("claude-{uid}"));
            let b = Path::new(&env).join("claude");
            return if !a.is_dir() && b.is_dir() { b } else { a };
        }
    }
    if cfg!(target_os = "macos") {
        return PathBuf::from(format!("/private/tmp/claude-{uid}"));
    }
    for base in [std::env::var("TMPDIR").unwrap_or_default(), "/tmp".to_string()] {
        if !base.is_empty() {
            let c = Path::new(&base).join(format!("claude-{uid}"));
            if c.is_dir() {
                return c;
            }
        }
    }
    PathBuf::from(format!("/tmp/claude-{uid}"))
}

impl Env {
    pub fn new() -> Env {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        let cfg = std::env::var("CLAUDE_CONFIG_DIR").ok().filter(|s| !s.is_empty());
        let claude = match &cfg {
            Some(c) => PathBuf::from(expand(c, &home)),
            None => home.join(".claude"),
        };
        let claude_json = if cfg.is_some() && claude.join(".claude.json").exists() {
            claude.join(".claude.json")
        } else {
            home.join(".claude.json")
        };
        let app = home.join("Library/Application Support/Claude");
        let managed = if cfg!(target_os = "macos") {
            PathBuf::from("/Library/Application Support/ClaudeCode/managed-settings.json")
        } else {
            PathBuf::from("/etc/claude-code/managed-settings.json")
        };
        let support = home.join("Library/Application Support/ClaudeBackupTool");
        Env { tmp: find_tmp_root(), home, claude, claude_json, app, managed, support }
    }

    /// Putanja zapisana tako da radi i na drugom Macu (~ i $TMP umesto korisničkog imena).
    pub fn token(&self, p: &Path) -> String {
        let s = p.to_string_lossy().to_string();
        for (base, tok) in [(&self.tmp, "$TMP"), (&self.home, "~")] {
            let b = base.to_string_lossy();
            if s == b {
                return tok.into();
            }
            if s.starts_with(&format!("{}/", b)) {
                return format!("{}{}", tok, &s[b.len()..]);
            }
        }
        s
    }

    pub fn from_token(&self, tok: &str) -> PathBuf {
        if tok == "$TMP" || tok.starts_with("$TMP/") {
            return PathBuf::from(format!("{}{}", self.tmp.to_string_lossy(), &tok[4..]));
        }
        if tok == "~" || tok.starts_with("~/") {
            return PathBuf::from(format!("{}{}", self.home.to_string_lossy(), &tok[1..]));
        }
        PathBuf::from(tok)
    }

    pub fn tilde(&self, p: &Path) -> String {
        let s = p.to_string_lossy().to_string();
        let h = self.home.to_string_lossy().to_string();
        if s == h || s.starts_with(&format!("{}/", h)) {
            format!("~{}", &s[h.len()..])
        } else {
            s
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

pub fn expand(p: &str, home: &Path) -> String {
    if p == "~" {
        return home.to_string_lossy().into();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return home.join(rest).to_string_lossy().into();
    }
    p.into()
}

/// Ima li `p` ispod `base` (ili je isto)?
pub fn is_within(p: &Path, base: &Path) -> bool {
    let (ps, bs) = (p.to_string_lossy(), base.to_string_lossy());
    ps == bs || ps.starts_with(&format!("{}/", bs.trim_end_matches('/')))
}
