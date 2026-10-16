//! Čitljiva verzija razgovora: HTML (Pregled.html) i Markdown. Original se nikad ne menja.

use crate::env::Env;
use crate::scan::{load_json, uuid_re, FEntry, Inventory};
use crate::util::*;
use base64::Engine;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const CSS: &str = include_str!("style.css");

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text,
    Thinking,
    Tool,
    Image,
    Command,
    Output,
    Notice,
    Summary,
    Raw,
}

#[derive(Clone, Debug)]
pub enum Part {
    Text(String),
    Image(String, String),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    pub text: String,
    pub name: String,
    pub data: Option<Value>,
    pub image: Option<(String, String)>,
    pub result: Vec<Part>,
    pub is_error: bool,
}

impl Item {
    fn new(kind: Kind, text: impl Into<String>) -> Item {
        Item { kind, text: text.into(), name: String::new(), data: None, image: None, result: vec![], is_error: false }
    }
}

#[derive(Clone, Debug)]
pub struct Turn {
    pub role: String,
    pub ts: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default)]
pub struct Conv {
    pub sid: String,
    pub src: PathBuf,
    pub kind: String,
    pub proj: String,
    pub arc: String,
    pub title: String,
    pub first: String,
    pub cwd: String,
    pub start: String,
    pub end: String,
    pub models: Vec<String>,
    pub entry: String,
    pub version: String,
    pub turns: Vec<Turn>,
    pub bad: u64,
}

fn re(pat: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

fn reminder_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"(?s)<system-reminder>.*?</system-reminder>", &R)
}

fn ansi_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"\x1b\[[0-9;?]*[ -/]*[@-~]", &R)
}

fn strip_reminders(s: &str) -> String {
    reminder_re().replace_all(s, "").to_string()
}

fn tag_re(tag: &str) -> Regex {
    Regex::new(&format!(r"(?s)<{tag}>(.*?)</{tag}>")).unwrap()
}

fn image_of(b: &Value) -> Option<(String, String)> {
    let s = b.get("source")?;
    if s.get("type")?.as_str()? == "base64" {
        let data = s.get("data")?.as_str()?;
        if !data.is_empty() {
            let mt = s.get("media_type").and_then(|m| m.as_str()).unwrap_or("image/png");
            return Some((mt.to_string(), data.to_string()));
        }
    }
    None
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn result_parts(content: Option<&Value>) -> Vec<Part> {
    match content {
        None | Some(Value::Null) => vec![],
        Some(Value::String(s)) => vec![Part::Text(s.clone())],
        Some(Value::Array(a)) => a
            .iter()
            .map(|b| match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => Part::Text(b.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()),
                Some("image") => match image_of(b) {
                    Some((m, d)) => Part::Image(m, d),
                    None => Part::Text("[slika]".into()),
                },
                _ => Part::Text(if b.is_string() { b.as_str().unwrap().to_string() } else { pretty(b) }),
            })
            .collect(),
        Some(other) => vec![Part::Text(pretty(other))],
    }
}

fn parse_user_text(text: &str) -> Vec<Item> {
    static CAVEAT: OnceLock<Regex> = OnceLock::new();
    static CMD: OnceLock<Regex> = OnceLock::new();
    static ARGS: OnceLock<Regex> = OnceLock::new();
    let mut text = strip_reminders(text);
    text = re(r"(?s)<local-command-caveat>.*?</local-command-caveat>", &CAVEAT).replace_all(&text, "").to_string();
    let mut items = vec![];
    if let Some(m) = re(r"(?s)<command-name>(.*?)</command-name>", &CMD).captures(&text) {
        let args = re(r"(?s)<command-args>(.*?)</command-args>", &ARGS)
            .captures(&text)
            .map(|a| a[1].trim().to_string())
            .unwrap_or_default();
        items.push(Item::new(Kind::Command, format!("{} {}", m[1].trim(), args).trim().to_string()));
        for t in ["command-name", "command-message", "command-args"] {
            text = tag_re(t).replace_all(&text, "").to_string();
        }
    }
    for tag in ["bash-input", "local-command-stdout", "local-command-stderr", "bash-stdout", "bash-stderr"] {
        let r = tag_re(tag);
        for c in r.captures_iter(&text) {
            let val = ansi_re().replace_all(&c[1], "").trim().to_string();
            if !val.is_empty() {
                items.push(if tag == "bash-input" { Item::new(Kind::Command, format!("! {val}")) } else { Item::new(Kind::Output, val) });
            }
        }
        text = r.replace_all(&text, "").to_string();
    }
    let t = text.trim();
    if !t.is_empty() {
        items.push(Item::new(Kind::Text, t));
    }
    items
}

pub fn parse_conv(path: &Path, kind: &str, proj: &str, titles: &HashMap<String, String>) -> Option<Conv> {
    let f = fs::File::open(path).ok()?;
    let mut c = Conv {
        sid: path.file_stem()?.to_string_lossy().to_string(),
        src: path.to_path_buf(),
        kind: kind.into(),
        proj: proj.into(),
        ..Default::default()
    };
    let (mut custom, mut ai, mut summary): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    let mut first_user = String::new();
    let mut tools: HashMap<String, (usize, usize)> = HashMap::new();

    fn cur(c: &mut Conv, role: &str, ts: &str) -> usize {
        if c.turns.last().map(|t| t.role == role).unwrap_or(false) {
            return c.turns.len() - 1;
        }
        c.turns.push(Turn { role: role.into(), ts: ts.into(), items: vec![] });
        c.turns.len() - 1
    }

    for raw in BufReader::new(f).split(b'\n') {
        let raw = match raw {
            Ok(r) => r,
            Err(_) => break,
        };
        let line = String::from_utf8_lossy(&raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let d: Value = match serde_json::from_str(line) {
            Ok(Value::Object(o)) => Value::Object(o),
            _ => {
                c.bad += 1;
                continue;
            }
        };
        let s = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let t = s("type");
        if t == "custom-title" {
            let v = if !s("customTitle").is_empty() { s("customTitle") } else { s("title") };
            if !v.is_empty() {
                custom = Some(v);
            }
            continue;
        }
        if !s("aiTitle").trim().is_empty() {
            ai = Some(s("aiTitle"));
        }
        if t == "summary" {
            if summary.is_none() && !s("summary").is_empty() {
                summary = Some(s("summary"));
            }
            continue;
        }
        if c.cwd.is_empty() && !s("cwd").is_empty() {
            c.cwd = s("cwd");
        }
        if !s("version").is_empty() {
            c.version = s("version");
        }
        if c.entry.is_empty() && !s("entrypoint").is_empty() {
            c.entry = s("entrypoint");
        }
        if t != "user" && t != "assistant" && t != "system" {
            continue;
        }
        let ts = s("timestamp");
        if !ts.is_empty() {
            if c.start.is_empty() {
                c.start = ts.clone();
            }
            c.end = ts.clone();
        }
        if d.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if t == "system" {
            let st = s("subtype");
            if st == "compact_boundary" {
                c.turns.push(Turn {
                    role: "divider".into(),
                    ts: ts.clone(),
                    items: vec![Item::new(Kind::Notice, "Razgovor je sažet (compact) — Claude nastavlja na osnovu sažetka")],
                });
            } else if s("level") == "error" || s("level") == "warning" || st == "api_error" || st == "local_command" {
                let txt = strip_reminders(&s("content"));
                if !txt.trim().is_empty() {
                    let i = cur(&mut c, "assistant", &ts);
                    let mut it = Item::new(Kind::Notice, ansi_re().replace_all(&txt, "").trim().to_string());
                    it.is_error = s("level") == "error";
                    c.turns[i].items.push(it);
                }
            }
            continue;
        }
        if d.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let msg = match d.get("message") {
            Some(m) if m.is_object() => m,
            _ => continue,
        };
        let content: Vec<Value> = match msg.get("content") {
            Some(Value::String(st)) => vec![serde_json::json!({"type": "text", "text": st})],
            Some(Value::Array(a)) => a.clone(),
            _ => continue,
        };
        if t == "assistant" {
            if let Some(m) = msg.get("model").and_then(|m| m.as_str()) {
                if !m.is_empty() && m != "<synthetic>" && !c.models.contains(&m.to_string()) {
                    c.models.push(m.to_string());
                }
            }
            let api_err = d.get("isApiErrorMessage").and_then(|v| v.as_bool()).unwrap_or(false);
            let ti = cur(&mut c, "assistant", &ts);
            for b in &content {
                let bt = b.get("type").and_then(|x| x.as_str()).unwrap_or("");
                match bt {
                    "text" => {
                        let tx = b.get("text").and_then(|x| x.as_str()).unwrap_or("");
                        if !tx.trim().is_empty() {
                            let mut it = Item::new(Kind::Text, tx);
                            it.is_error = api_err;
                            c.turns[ti].items.push(it);
                        }
                    }
                    "thinking" => {
                        let tx = b.get("thinking").and_then(|x| x.as_str()).unwrap_or("");
                        if !tx.trim().is_empty() {
                            c.turns[ti].items.push(Item::new(Kind::Thinking, tx));
                        }
                    }
                    "redacted_thinking" => {}
                    "tool_use" | "server_tool_use" | "mcp_tool_use" => {
                        let mut it = Item::new(Kind::Tool, "");
                        it.name = b.get("name").and_then(|x| x.as_str()).unwrap_or("alat").to_string();
                        it.data = b.get("input").cloned();
                        c.turns[ti].items.push(it);
                        if let Some(id) = b.get("id").and_then(|x| x.as_str()) {
                            tools.insert(id.to_string(), (ti, c.turns[ti].items.len() - 1));
                        }
                    }
                    _ if bt.ends_with("_tool_result") => {
                        let parts = result_parts(b.get("content"));
                        let id = b.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("");
                        if let Some(&(a, i)) = tools.get(id) {
                            c.turns[a].items[i].result.extend(parts);
                        } else {
                            let mut it = Item::new(Kind::Tool, "");
                            it.name = bt.to_string();
                            it.result = parts;
                            c.turns[ti].items.push(it);
                        }
                    }
                    _ => {
                        let mut it = Item::new(Kind::Raw, pretty(b));
                        it.name = if bt.is_empty() { "podaci".into() } else { bt.into() };
                        c.turns[ti].items.push(it);
                    }
                }
            }
        } else {
            if d.get("isCompactSummary").and_then(|v| v.as_bool()).unwrap_or(false) {
                let txt: Vec<String> = content
                    .iter()
                    .filter(|b| b.get("type").and_then(|x| x.as_str()) == Some("text"))
                    .map(|b| b.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string())
                    .collect();
                c.turns.push(Turn {
                    role: "divider".into(),
                    ts: ts.clone(),
                    items: vec![Item::new(Kind::Summary, strip_reminders(&txt.join("\n")).trim().to_string())],
                });
                continue;
            }
            for b in &content {
                let bt = b.get("type").and_then(|x| x.as_str()).unwrap_or("");
                match bt {
                    "tool_result" => {
                        let parts = result_parts(b.get("content"));
                        let err = b.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false);
                        let id = b.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("");
                        if let Some(&(a, i)) = tools.get(id) {
                            let it = &mut c.turns[a].items[i];
                            it.result.extend(parts);
                            it.is_error = it.is_error || err;
                        } else {
                            let ti = cur(&mut c, "assistant", &ts);
                            let mut it = Item::new(Kind::Tool, "");
                            it.name = "rezultat alata".into();
                            it.result = parts;
                            it.is_error = err;
                            c.turns[ti].items.push(it);
                        }
                    }
                    "text" => {
                        let its = parse_user_text(b.get("text").and_then(|x| x.as_str()).unwrap_or(""));
                        if !its.is_empty() {
                            if first_user.is_empty() {
                                if let Some(i) = its.iter().find(|i| i.kind == Kind::Text || i.kind == Kind::Command) {
                                    first_user = i.text.clone();
                                }
                            }
                            let ti = cur(&mut c, "user", &ts);
                            c.turns[ti].items.extend(its);
                        }
                    }
                    "image" => {
                        let ti = cur(&mut c, "user", &ts);
                        let it = match image_of(b) {
                            Some(img) => {
                                let mut i = Item::new(Kind::Image, "");
                                i.image = Some(img);
                                i
                            }
                            None => Item::new(Kind::Notice, "[slika]"),
                        };
                        c.turns[ti].items.push(it);
                    }
                    "document" => {
                        let ti = cur(&mut c, "user", &ts);
                        let title = b.get("title").and_then(|x| x.as_str()).map(|t| format!(": {t}")).unwrap_or_default();
                        c.turns[ti].items.push(Item::new(Kind::Notice, format!("[priložen dokument{title}]")));
                    }
                    _ => {
                        let ti = cur(&mut c, "user", &ts);
                        let mut it = Item::new(Kind::Raw, pretty(b));
                        it.name = bt.to_string();
                        c.turns[ti].items.push(it);
                    }
                }
            }
        }
    }
    c.turns.retain(|t| !t.items.is_empty());
    if !c.turns.iter().any(|t| t.role == "user" || t.role == "assistant") {
        return None;
    }
    c.first = first_user.trim().to_string();
    let first_line: String = c.first.lines().next().unwrap_or("").chars().take(90).collect();
    let title = custom
        .or_else(|| titles.get(&c.sid).cloned())
        .or(ai)
        .or(summary)
        .unwrap_or_else(|| if first_line.is_empty() { "(bez naslova)".into() } else { first_line });
    c.title = title.trim().to_string();
    Some(c)
}

/// Naslovi razgovora onako kako ih prikazuje Claude aplikacija (Code tab / Cowork kartice).
pub fn load_card_titles(dirs: &[PathBuf]) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    fn rec(d: &Path, depth: usize, titles: &mut HashMap<String, String>) {
        let rd = match fs::read_dir(d) {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut ents: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let p = e.path();
            let md = match fs::metadata(&p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                if depth < 4 {
                    rec(&p, depth + 1, titles);
                }
            } else if p.extension().map(|x| x == "json").unwrap_or(false) && md.len() <= 2_000_000 {
                if let Some(v) = load_json(&p) {
                    if let Some(t) = v.get("title").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()) {
                        for k in ["cliSessionId", "sessionId"] {
                            if let Some(id) = v.get(k).and_then(|x| x.as_str()) {
                                titles.entry(id.to_string()).or_insert_with(|| t.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    for d in dirs {
        rec(d, 0, &mut titles);
    }
    titles
}

pub fn source_label(c: &Conv) -> String {
    if c.kind == "cowork" {
        return "Cowork".into();
    }
    let e = c.entry.to_lowercase();
    let s = if e.contains("desktop") {
        "Code tab"
    } else if e == "cli" {
        "Terminal"
    } else if ["vscode", "jetbrains", "ide"].iter().any(|x| e.contains(x)) {
        "Editor"
    } else if e.contains("cowork") {
        "Cowork"
    } else if e.contains("remote") || e.contains("web") {
        "Cloud"
    } else if e.contains("sdk") {
        "SDK"
    } else if e.is_empty() {
        "—"
    } else {
        return c.entry.clone();
    };
    s.into()
}

pub fn tool_hint(inp: &Option<Value>) -> String {
    if let Some(Value::Object(o)) = inp {
        for k in ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt", "skill", "notebook_path"] {
            if let Some(v) = o.get(k).and_then(|v| v.as_str()) {
                if !v.trim().is_empty() {
                    let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
                    return if v.chars().count() <= 110 { v } else { format!("{}…", v.chars().take(110).collect::<String>()) };
                }
            }
        }
    }
    String::new()
}

pub fn fence(text: &str, lang: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let f = "`".repeat((longest + 1).max(3));
    format!("{f}{lang}\n{text}\n{f}")
}

// ---------------------------------------------------------------- HTML

fn inline_md(s: &str) -> String {
    static CODE: OnceLock<Regex> = OnceLock::new();
    static BOLD: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    let s = re(r"`([^`\n]+)`", &CODE).replace_all(s, "<code>$1</code>");
    let s = re(r"\*\*([^*\n]+)\*\*", &BOLD).replace_all(&s, "<strong>$1</strong>");
    re(r"\[([^\]\n]+)\]\((https?://[^)\s]+)\)", &LINK).replace_all(&s, "<a href=\"$2\">$1</a>").to_string()
}

/// Lagan prikaz markdowna: kod blokovi, `kod`, **bold**, naslovi. Tekst ostaje doslovno isti.
pub fn md_to_html(text: &str) -> String {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    let head = re(r"^(#{1,6}) (.*)$", &HEAD);
    let mut out = String::new();
    let mut buf: Vec<&str> = vec![];
    let mut code: Option<Vec<&str>> = None;
    let mut fence_mark = String::new();
    let flush = |buf: &mut Vec<&str>, out: &mut String| {
        if buf.is_empty() {
            return;
        }
        let joined = esc(&buf.join("\n"));
        let lines: Vec<String> = joined
            .split('\n')
            .map(|ln| match head.captures(ln) {
                Some(m) => format!("<span class=\"h h{}\">{}</span>", m[1].len(), inline_md(&m[2])),
                None => inline_md(ln),
            })
            .collect();
        out.push_str("<div class=\"md\">");
        out.push_str(&lines.join("\n"));
        out.push_str("</div>");
        buf.clear();
    };
    for ln in text.split('\n') {
        let s = ln.trim_start();
        if code.is_none() && s.starts_with("```") {
            flush(&mut buf, &mut out);
            fence_mark = s.chars().take_while(|c| *c == '`').collect();
            code = Some(vec![]);
        } else if code.is_some() && s.starts_with(fence_mark.as_str()) && s.trim_matches('`').trim().is_empty() {
            let c = code.take().unwrap_or_default();
            out.push_str(&format!("<pre><code>{}</code></pre>", esc(&c.join("\n"))));
        } else if let Some(c) = code.as_mut() {
            c.push(ln);
        } else {
            buf.push(ln);
        }
    }
    if let Some(c) = code {
        out.push_str(&format!("<pre><code>{}</code></pre>", esc(&c.join("\n"))));
    }
    flush(&mut buf, &mut out);
    out
}

fn html_input(inp: &Value) -> String {
    match inp {
        Value::Object(o) => o
            .iter()
            .map(|(k, v)| match v {
                Value::String(s) if s.contains('\n') || s.chars().count() > 90 => {
                    format!("<div class=\"kv\"><span>{}:</span></div><pre>{}</pre>", esc(k), esc(s))
                }
                Value::String(s) => format!("<div class=\"kv\"><span>{}:</span> <code>{}</code></div>", esc(k), esc(s)),
                other => format!("<div class=\"kv\"><span>{}:</span></div><pre>{}</pre>", esc(k), esc(&pretty(other))),
            })
            .collect(),
        other => format!("<pre>{}</pre>", esc(&pretty(other))),
    }
}

fn html_img(mt: &str, data: &str) -> String {
    format!("<img class=\"shot\" alt=\"slika\" src=\"data:{};base64,{}\">", esc(mt), data)
}

fn html_item(it: &Item) -> String {
    match it.kind {
        Kind::Text => {
            if it.is_error {
                format!("<div class=\"err-text\">{}</div>", md_to_html(&it.text))
            } else {
                md_to_html(&it.text)
            }
        }
        Kind::Command => format!("<div class=\"cmd\">{}</div>", esc(&it.text)),
        Kind::Output => format!("<pre>{}</pre>", esc(&it.text)),
        Kind::Thinking => format!(
            "<details class=\"think\"><summary>Razmišljanje</summary><div class=\"body\"><div class=\"md\">{}</div></div></details>",
            esc(&it.text)
        ),
        Kind::Image => it.image.as_ref().map(|(m, d)| html_img(m, d)).unwrap_or_default(),
        Kind::Notice => format!("<div class=\"notice{}\">{}</div>", if it.is_error { " err" } else { "" }, esc(&it.text)),
        Kind::Summary => format!(
            "<details><summary>Sažetak ranijeg dela razgovora</summary><div class=\"body\">{}</div></details>",
            md_to_html(&it.text)
        ),
        Kind::Tool => {
            let hint = tool_hint(&it.data);
            let mut body = String::new();
            if let Some(d) = &it.data {
                body.push_str("<div class=\"label\">Ulaz</div>");
                body.push_str(&html_input(d));
            }
            if !it.result.is_empty() {
                body.push_str("<div class=\"label\">Rezultat</div>");
                for p in &it.result {
                    match p {
                        Part::Image(m, d) => body.push_str(&html_img(m, d)),
                        Part::Text(t) => body.push_str(&format!("<pre>{}</pre>", esc(t))),
                    }
                }
            }
            format!(
                "<details class=\"tool{}\"><summary><b>{}</b>{}{}</summary><div class=\"body\">{}</div></details>",
                if it.is_error { " err" } else { "" },
                esc(&it.name),
                if hint.is_empty() { String::new() } else { format!(" <code>{}</code>", esc(&hint)) },
                if it.is_error { " — greška" } else { "" },
                body
            )
        }
        Kind::Raw => format!(
            "<details><summary>{}</summary><div class=\"body\"><pre>{}</pre></div></details>",
            esc(if it.name.is_empty() { "podaci" } else { &it.name }),
            esc(&it.text)
        ),
    }
}

fn html_page(title: &str, body: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"sr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{}</style></head><body><div class=\"wrap\">{}</div></body></html>",
        esc(title), CSS, body
    )
    .into_bytes()
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}

pub fn resume_cmd(c: &Conv) -> String {
    if c.cwd.is_empty() {
        format!("claude --resume {}", c.sid)
    } else {
        format!("cd {} && claude --resume {}", shell_quote(&c.cwd), c.sid)
    }
}

fn count_role(c: &Conv, role: &str) -> u64 {
    c.turns.iter().filter(|t| t.role == role).count() as u64
}

pub fn html_conv(c: &Conv, back: &str, env: &Env) -> Vec<u8> {
    let mut meta: Vec<(String, String)> = vec![
        ("Projekat".into(), if c.cwd.is_empty() { "—".into() } else { env.tilde(Path::new(&c.cwd)) }),
        ("Početak".into(), fmt_ts(&c.start, true)),
        ("Poslednja poruka".into(), fmt_ts(&c.end, true)),
        ("Poruke".into(), format!("ti: {} · Claude: {}", count_role(c, "user"), count_role(c, "assistant"))),
        ("Izvor".into(), source_label(c)),
        ("Model".into(), if c.models.is_empty() { "—".into() } else { c.models.join(", ") }),
        ("Session ID".into(), c.sid.clone()),
        ("Original".into(), c.arc.clone()),
    ];
    if c.bad > 0 {
        let verb = plural_word(c.bad, "nije mogao", "nisu mogla", "nije moglo");
        meta.push(("Napomena".into(), format!(
            "{} u originalu {} da se prikaže (original je sačuvan netaknut)",
            plural(c.bad, "oštećen red", "oštećena reda", "oštećenih redova"), verb)));
    }
    let dl: String = meta.iter().map(|(k, v)| format!("<dt>{}</dt><dd>{}</dd>", esc(k), esc(v))).collect();
    let mut parts = vec![
        format!("<a class=\"back\" href=\"{}\">← Svi razgovori</a>", back),
        format!(
            "<header class=\"head\"><h1>{}</h1><dl class=\"meta\">{}</dl><div class=\"resume\">Nastavi ovaj razgovor u terminalu:<code>{}</code></div></header>",
            esc(&c.title), dl, esc(&resume_cmd(c))
        ),
    ];
    for t in &c.turns {
        if t.role == "divider" {
            for it in &t.items {
                parts.push(if it.kind == Kind::Summary { html_item(it) } else { format!("<div class=\"divider\">{}</div>", esc(&it.text)) });
            }
            continue;
        }
        let who = if t.role == "user" { "Ti" } else { "Claude" };
        let inner: String = t.items.iter().map(html_item).collect();
        parts.push(format!(
            "<section class=\"turn {}\"><div class=\"who\"><span>{}</span><time>{}</time></div><div class=\"bubble\">{}</div></section>",
            t.role, who, esc(&fmt_ts(&t.ts, true)), inner
        ));
    }
    html_page(&c.title, &parts.concat())
}

pub fn html_index(groups: &[(String, Vec<(&Conv, String)>)], created: &str, has_md: bool) -> Vec<u8> {
    let convs: Vec<&Conv> = groups.iter().flat_map(|(_, l)| l.iter().map(|(c, _)| *c)).collect();
    let mut starts: Vec<_> = convs.iter().filter_map(|c| parse_ts(&c.start)).collect();
    starts.sort();
    let period = match (starts.first(), starts.last()) {
        (Some(a), Some(b)) => format!("{} – {}", a.format("%d.%m.%Y."), b.format("%d.%m.%Y.")),
        _ => "—".into(),
    };
    let msgs: u64 = convs.iter().map(|c| count_role(c, "user")).sum();
    let tiles = format!(
        "<div class=\"tiles\"><div class=\"tile\"><b>{}</b><span>razgovora</span></div><div class=\"tile\"><b>{}</b><span>projekata</span></div><div class=\"tile\"><b>{}</b><span>tvojih poruka</span></div><div class=\"tile\"><b style=\"font-size:15px\">{}</b><span>period</span></div></div>",
        num(convs.len() as u64), num(groups.len() as u64), num(msgs), esc(&period)
    );
    let mut rows = String::new();
    for (label, lst) in groups {
        let mut trs = String::new();
        for (c, href) in lst {
            let snippet: String = c.first.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(180).collect();
            let s = esc(&format!("{} {} {}", c.title, snippet, label).to_lowercase());
            let sub = if !snippet.is_empty() && snippet != c.title { format!("<div class=sub>{}</div>", esc(&snippet)) } else { String::new() };
            trs.push_str(&format!(
                "<tr data-s=\"{}\"><td class=\"d\">{}</td><td><a href=\"{}\">{}</a><span class=\"badge\">{}</span>{}</td><td class=\"n\">{} poruka</td></tr>",
                s, esc(&fmt_ts(&c.start, false)), esc(href), esc(&c.title), esc(&source_label(c)), sub, count_role(c, "user")
            ));
        }
        rows.push_str(&format!(
            "<details class=\"proj\" open><summary><b>{}</b> <span>· {}</span></summary><table>{}</table></details>",
            esc(label), n_convs(lst.len() as u64), trs
        ));
    }
    let js = "<script>const q=document.getElementById('q');q.addEventListener('input',()=>{const v=q.value.trim().toLowerCase();document.querySelectorAll('.proj').forEach(g=>{let any=false;g.querySelectorAll('tr[data-s]').forEach(r=>{const m=!v||r.dataset.s.includes(v);r.hidden=!m;if(m)any=true;});g.hidden=!any;if(v&&any)g.open=true;});});</script>";
    let body = format!(
        "<h1>Claude razgovori</h1><div class=\"note\">Backup napravljen {}. Originalni Claude fajlovi su u folderu <code>original/</code> — vidi PROCITAJ_ME.txt.{}</div>{}<input id=\"q\" type=\"search\" placeholder=\"Pretraži naslove, poruke i projekte…\" autocomplete=\"off\">{}{}",
        esc(created),
        if has_md { " Markdown verzija je u folderu <code>citljivo-md/</code>." } else { "" },
        tiles,
        if rows.is_empty() { "<p class=note>Nema razgovora u ovom backupu.</p>".to_string() } else { rows },
        js
    );
    html_page("Claude razgovori", &body)
}

// ---------------------------------------------------------------- Markdown

fn md_input(inp: &Value) -> String {
    match inp {
        Value::Object(o) => o
            .iter()
            .map(|(k, v)| match v {
                Value::String(s) if s.contains('\n') || s.chars().count() > 90 => format!("*{}:*\n\n{}", k, fence(s, "")),
                Value::String(s) if s.contains('`') => format!("*{}:* {}", k, s),
                Value::String(s) => format!("*{}:* `{}`", k, s),
                other => format!("*{}:*\n\n{}", k, fence(&pretty(other), "json")),
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        other => fence(&pretty(other), "json"),
    }
}

pub fn md_conv(c: &Conv, env: &Env, put_image: &mut dyn FnMut(&str, &str) -> String) -> Vec<u8> {
    let mut l: Vec<String> = vec![
        format!("# {}", c.title),
        String::new(),
        "| | |".into(),
        "|---|---|".into(),
        format!("| Projekat | `{}` |", if c.cwd.is_empty() { "—".into() } else { env.tilde(Path::new(&c.cwd)) }),
        format!("| Početak | {} |", fmt_ts(&c.start, true)),
        format!("| Poslednja poruka | {} |", fmt_ts(&c.end, true)),
        format!("| Izvor | {} |", source_label(c)),
        format!("| Model | {} |", if c.models.is_empty() { "—".into() } else { c.models.join(", ") }),
        format!("| Session ID | `{}` |", c.sid),
        format!("| Original | `{}` |", c.arc),
        String::new(),
        "Nastavi u terminalu:".into(),
        String::new(),
        fence(&resume_cmd(c), "bash"),
        String::new(),
        "---".into(),
        String::new(),
    ];
    for t in &c.turns {
        if t.role == "divider" {
            for it in &t.items {
                if it.kind == Kind::Summary {
                    l.extend(["<details><summary>Sažetak ranijeg dela razgovora</summary>".into(), String::new(), it.text.clone(), String::new(), "</details>".into(), String::new()]);
                } else {
                    l.extend([format!("--- *{}* ---", it.text), String::new()]);
                }
            }
            continue;
        }
        l.extend([format!("## {} · {}", if t.role == "user" { "🧑 Ti" } else { "🤖 Claude" }, fmt_ts(&t.ts, true)), String::new()]);
        for it in &t.items {
            match it.kind {
                Kind::Text => l.extend([it.text.clone(), String::new()]),
                Kind::Command => l.extend([
                    if !it.text.contains('`') && !it.text.contains('\n') { format!("`{}`", it.text) } else { fence(&it.text, "") },
                    String::new(),
                ]),
                Kind::Output => l.extend([fence(&it.text, ""), String::new()]),
                Kind::Thinking => l.extend(["<details><summary>Razmišljanje</summary>".into(), String::new(), it.text.clone(), String::new(), "</details>".into(), String::new()]),
                Kind::Image => {
                    if let Some((m, d)) = &it.image {
                        let rel = put_image(m, d);
                        l.extend([format!("![slika]({rel})"), String::new()]);
                    }
                }
                Kind::Notice => l.extend([format!("*{}*", it.text), String::new()]),
                Kind::Tool => {
                    let hint = tool_hint(&it.data);
                    let summ = format!(
                        "🔧 {}{}{}",
                        it.name,
                        if hint.is_empty() { String::new() } else { format!(" — {}", esc(&hint)) },
                        if it.is_error { " — greška" } else { "" }
                    );
                    l.extend([format!("<details><summary>{summ}</summary>"), String::new()]);
                    if let Some(d) = &it.data {
                        l.extend(["**Ulaz**".into(), String::new(), md_input(d), String::new()]);
                    }
                    if !it.result.is_empty() {
                        l.extend(["**Rezultat**".into(), String::new()]);
                        for p in &it.result {
                            match p {
                                Part::Image(m, d) => {
                                    let rel = put_image(m, d);
                                    l.extend([format!("![slika]({rel})"), String::new()]);
                                }
                                Part::Text(t) => l.extend([fence(t, ""), String::new()]),
                            }
                        }
                    }
                    l.extend(["</details>".into(), String::new()]);
                }
                Kind::Summary | Kind::Raw => l.extend([
                    format!("<details><summary>{}</summary>", if it.name.is_empty() { "podaci" } else { &it.name }),
                    String::new(),
                    fence(&it.text, "json"),
                    String::new(),
                    "</details>".into(),
                    String::new(),
                ]),
            }
        }
    }
    l.join("\n").into_bytes()
}

// ---------------------------------------------------------------- sve zajedno

pub fn build_readable(
    inv: &Inventory,
    entries: &[FEntry],
    html: bool,
    md: bool,
    gen: &mut dyn FnMut(&str, Vec<u8>) -> io::Result<()>,
    rep: &Reporter,
) -> io::Result<usize> {
    let env = &inv.env;
    let mut srcs: Vec<(&FEntry, &str)> = vec![];
    for e in entries {
        let n = e.name();
        if !n.ends_with(".jsonl") || !uuid_re().is_match(&n[..n.len() - 6]) {
            continue;
        }
        if e.cat == "code_sessions" && e.arc.matches('/').count() == 4 {
            srcs.push((e, "code"));
        } else if e.cat == "cowork" {
            srcs.push((e, "cowork"));
        }
    }
    if srcs.is_empty() {
        return Ok(0);
    }
    let titles = load_card_titles(&[env.app.join("claude-code-sessions"), env.app.join("local-agent-mode-sessions")]);
    let mut convs: Vec<Conv> = vec![];
    let total = srcs.len();
    for (i, (e, kind)) in srcs.iter().enumerate() {
        rep.emit("read", i as f64 / total as f64, format!("{}/{}", i + 1, total), e.name(), false);
        let proj = if *kind == "code" { e.proj.clone() } else { "cowork".into() };
        if let Some(mut c) = parse_conv(&e.src, kind, &proj, &titles) {
            c.arc = e.arc.clone();
            convs.push(c);
        }
    }
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, c) in convs.iter().enumerate() {
        groups.entry(c.proj.clone()).or_default().push(i);
    }
    let mut labels: HashMap<String, String> = HashMap::new();
    let mut dirs: HashMap<String, String> = HashMap::new();
    let mut used = HashSet::new();
    for (key, idxs) in &groups {
        let label = if key == "cowork" {
            "Cowork".to_string()
        } else {
            let cwd = inv.projects.get(key).map(|p| p.cwd.clone()).filter(|c| !c.is_empty())
                .or_else(|| idxs.iter().map(|&i| convs[i].cwd.clone()).find(|c| !c.is_empty()))
                .unwrap_or_default();
            if cwd.is_empty() { key.clone() } else { env.tilde(Path::new(&cwd)) }
        };
        let base_name = if label == "Cowork" { "cowork".to_string() } else {
            Path::new(&label).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(label.clone())
        };
        let base = slug(&base_name, 40);
        let (mut d, mut k) = (base.clone(), 2);
        while used.contains(&d) {
            d = format!("{base}-{k}");
            k += 1;
        }
        used.insert(d.clone());
        labels.insert(key.clone(), label);
        dirs.insert(key.clone(), d);
    }
    let mut order: Vec<&String> = groups.keys().collect();
    let last_of = |k: &String| -> String {
        groups[k].iter().map(|&i| if convs[i].end.is_empty() { convs[i].start.clone() } else { convs[i].end.clone() }).max().unwrap_or_default()
    };
    order.sort_by_key(|k| std::cmp::Reverse(last_of(k)));
    let mut html_groups: Vec<(String, Vec<(&Conv, String)>)> = vec![];
    let mut md_rows: Vec<(String, &Conv, String)> = vec![];
    let ng = order.len().max(1);
    for (gi, key) in order.iter().enumerate() {
        let mut idxs = groups[*key].clone();
        idxs.sort_by_key(|&i| std::cmp::Reverse(convs[i].start.clone()));
        let mut hg = vec![];
        for i in idxs {
            let c = &convs[i];
            let mut fname = parse_ts(&c.start).map(|d| d.format("%Y-%m-%d_").to_string()).unwrap_or_default();
            fname.push_str(&format!("{}_{}", slug(&c.title, 50), &c.sid[..8.min(c.sid.len())]));
            rep.emit("render", gi as f64 / ng as f64, labels[*key].clone(), c.title.clone(), false);
            let dir = &dirs[*key];
            if html {
                let href = format!("citljivo-html/{dir}/{fname}.html");
                gen(&href, html_conv(c, "../../Pregled.html", env))?;
                hg.push((c, href));
            }
            if md {
                let mut counter = 0;
                let sid8: String = c.sid.chars().take(8).collect();
                let mut imgs: Vec<(String, Vec<u8>)> = vec![];
                let mut put = |media: &str, data: &str| -> String {
                    counter += 1;
                    let ext = match media {
                        "image/jpeg" => "jpg",
                        "image/gif" => "gif",
                        "image/webp" => "webp",
                        _ => "png",
                    };
                    let rel = format!("slike/{sid8}-{counter}.{ext}");
                    match base64::engine::general_purpose::STANDARD.decode(data.trim()) {
                        Ok(bytes) => {
                            imgs.push((format!("citljivo-md/{dir}/{rel}"), bytes));
                            rel
                        }
                        Err(_) => "(slika nije mogla da se dekodira)".into(),
                    }
                };
                let body = md_conv(c, env, &mut put);
                for (arc, bytes) in imgs {
                    gen(&arc, bytes)?;
                }
                gen(&format!("citljivo-md/{dir}/{fname}.md"), body)?;
                md_rows.push((labels[*key].clone(), c, format!("{dir}/{fname}.md")));
            }
        }
        if !hg.is_empty() {
            html_groups.push((labels[*key].clone(), hg));
        }
    }
    let created = chrono::Local::now().format("%d.%m.%Y. u %H:%M").to_string();
    if html {
        gen("Pregled.html", html_index(&html_groups, &created, md))?;
    }
    if md {
        let mut l = vec!["# Claude razgovori".to_string(), String::new(), format!("{} · backup napravljen {}", n_convs(convs.len() as u64), created), String::new()];
        let mut last = String::new();
        for (label, c, rel) in &md_rows {
            if *label != last {
                l.extend([String::new(), format!("## {label}"), String::new(), "| Datum | Razgovor | Izvor | Poruka |".into(), "|---|---|---|---|".into()]);
                last = label.clone();
            }
            l.push(format!("| {} | [{}]({}) | {} | {} |", fmt_ts(&c.start, false), c.title.replace('|', "/"), rel.replace(' ', "%20"), source_label(c), count_role(c, "user")));
        }
        gen("citljivo-md/INDEX.md", (l.join("\n") + "\n").into_bytes())?;
    }
    Ok(convs.len())
}
