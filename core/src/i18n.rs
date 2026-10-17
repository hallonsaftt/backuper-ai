//! Language for everything the program says: English (default) or Serbian.
//! Jezik svih poruka: engleski (podrazumevano) ili srpski.
//!
//! The choice is saved in the program's support folder (prefs.json), so the app,
//! scheduled backups and the command line all speak the same language.

use crate::env::Env;
use std::sync::atomic::{AtomicBool, Ordering};

static SR: AtomicBool = AtomicBool::new(false);

pub fn set_lang(code: &str) {
    SR.store(code.trim().eq_ignore_ascii_case("sr"), Ordering::Relaxed);
}

/// "en" or "sr"
pub fn lang() -> &'static str {
    if sr() {
        "sr"
    } else {
        "en"
    }
}

pub fn sr() -> bool {
    SR.load(Ordering::Relaxed)
}

/// Picks the text for the current language.
pub fn t(sr_text: &'static str, en_text: &'static str) -> &'static str {
    if sr() {
        sr_text
    } else {
        en_text
    }
}

/// `tf!("srpski {}", "english {}", x)` — format! in the current language.
#[macro_export]
macro_rules! tf {
    ($sr:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::sr() { format!($sr $(, $a)*) } else { format!($en $(, $a)*) }
    };
}

fn prefs_path(env: &Env) -> std::path::PathBuf {
    env.support.join("prefs.json")
}

/// Saved language, or "en" if nothing was chosen yet.
pub fn saved_lang(env: &Env) -> String {
    crate::scan::load_json(&prefs_path(env))
        .and_then(|v| v.get("lang").and_then(|l| l.as_str()).map(|s| s.to_string()))
        .filter(|l| l == "en" || l == "sr")
        .unwrap_or_else(|| "en".into())
}

/// Loads the saved language and makes it current.
pub fn init(env: &Env) {
    set_lang(&saved_lang(env));
}

/// Saves and applies a new language. Writes only our own prefs.json.
pub fn save_lang(env: &Env, code: &str) -> std::io::Result<()> {
    let code = if code.eq_ignore_ascii_case("sr") { "sr" } else { "en" };
    std::fs::create_dir_all(&env.support)?;
    let p = prefs_path(env);
    let mut v = crate::scan::load_json(&p).filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}));
    v["lang"] = serde_json::Value::from(code);
    let tmp = p.with_file_name("prefs.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&v).unwrap())?;
    std::fs::rename(&tmp, &p)?;
    set_lang(code);
    Ok(())
}

/// Names of the human-readable files inside a backup (the format itself is the same in both languages).
pub struct Names {
    pub readme: &'static str,
    pub report: &'static str,
    pub overview: &'static str,
    pub html_dir: &'static str,
    pub md_dir: &'static str,
    pub prompts: &'static str,
    pub project_dir: &'static str,
}

pub fn names() -> Names {
    if sr() {
        Names {
            readme: "PROCITAJ_ME.txt",
            report: "izvestaj.txt",
            overview: "Pregled.html",
            html_dir: "citljivo-html",
            md_dir: "citljivo-md",
            prompts: "kucane-poruke.jsonl",
            project_dir: "claude-istorija",
        }
    } else {
        Names {
            readme: "README.txt",
            report: "report.txt",
            overview: "Overview.html",
            html_dir: "readable-html",
            md_dir: "readable-md",
            prompts: "typed-prompts.jsonl",
            project_dir: "claude-history",
        }
    }
}

/// Both project-history folder names (old backups may be in either).
pub const PROJECT_DIRS: [&str; 2] = ["claude-history", "claude-istorija"];

/// Tests that switch the language take this lock, so they don't race each other.
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_lang("sr");
        assert_eq!(t("da", "yes"), "da");
        assert_eq!(tf!("{} fajla", "{} files", 2), "2 fajla");
        set_lang("en");
        assert_eq!(t("da", "yes"), "yes");
        assert_eq!(tf!("{} fajla", "{} files", 2), "2 files");
        assert_eq!(lang(), "en");
    }
}
