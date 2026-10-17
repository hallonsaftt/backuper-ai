// Lažni podaci za pregled interfejsa u običnom pregledaču (bez Tauri-ja). U aplikaciji se ne koristi.
import { isWin } from "./lib/platform";
import type { BackupFile, Overview, Proj, Sched } from "./types";

/** U pregledaču sa ?os=windows: putanje kao na Windowsu */
const PATH_KEYS = new Set(["cwd", "cwd_tilde", "path", "path_tilde", "dir_tilde", "dest", "dest_tilde", "log", "home", "app_path", "program", "describe"]);
const APP_WIN = "C:\\Users\\petar\\AppData\\Local\\Claude Backup\\claude-backup.exe";
function winify<T>(v: T): T {
  if (!isWin) return v;
  if (Array.isArray(v)) return v.map(winify) as T;
  if (v && typeof v === "object") {
    const o: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) {
      if (typeof x === "string" && PATH_KEYS.has(k)) {
        if (k === "program" || k === "app_path") o[k] = APP_WIN;
        else if (k === "log") o[k] = "C:\\Users\\petar\\AppData\\Roaming\\ClaudeBackupTool\\claude-backup.log";
        else if (k === "describe") o[k] = x.replace(/~\/ClaudeBackups/, "C:\\Users\\petar\\ClaudeBackups");
        else o[k] = W(x);
      } else o[k] = winify(x);
    }
    return o as T;
  }
  return v;
}
const W = (p: string) => (isWin ? p.replace(/^~\//, "C:\\Users\\petar\\").replace(/^\/Users\/petar/, "C:\\Users\\petar").replace(/\//g, "\\") : p);

type Emit = (ev: "job-progress" | "job-done", payload: unknown) => void;
let emit: Emit = () => {};
export const setEmitter = (e: Emit) => {
  emit = e;
};

const now = Date.now() / 1000;
const day = 86400;
const P = (name: string, dir: string, sessions: number, size: number, lastDays: number): Proj => ({
  key: `-Users-petar-${dir.replace(/[^A-Za-z0-9]/g, "-")}-${name}`,
  cwd: `/Users/petar/${dir}/${name}`,
  cwd_tilde: `~/${dir}/${name}`,
  name,
  sessions,
  size,
  last: now - lastDays * day,
  last_text: new Date((now - lastDays * day) * 1000).toLocaleDateString("sr-Latn-RS"),
  exists: true,
  has_files: sessions > 4,
});

const projects: Proj[] = [
  P("pos-cafe", "Projects", 48, 212_400_000, 0.2),
  P("Claude Backupmaker", "Desktop", 6, 18_900_000, 0.9),
  P("fakture-api", "Projects", 31, 96_200_000, 3),
  P("klijent-dashboard", "Documents", 22, 61_300_000, 9),
  P("mobilna-aplikacija", "Projects", 17, 44_800_000, 16),
  P("landing-2026", "Projects", 9, 12_100_000, 27),
  P("skripte", "Projects", 4, 2_300_000, 54),
];

let schedule: Sched = {
  enabled: true,
  broken: false,
  freq: "daily",
  hour: 14,
  minute: 0,
  dest: "/Users/petar/ClaudeBackups",
  dest_tilde: "~/ClaudeBackups",
  keep: 14,
  loaded: true,
  program: "/Applications/Claude Backup.app/Contents/MacOS/claude-backup",
  describe: "svakog dana u 14:00 → ~/ClaudeBackups · čuva 14 poslednjih backupa",
  last_run: { time: new Date((now - 0.9 * day) * 1000).toISOString(), ok: true, message: "1.284 fajla · 512,3 MB · provereno", path: "~/ClaudeBackups/ClaudeBackup_2026-10-16_1400.zip" },
  log: "~/Library/Logs/claude-backup.log",
};

let cleanup: number | null = null;

const backups: BackupFile[] = [
  { path: "/Users/petar/ClaudeBackups/ClaudeBackup_2026-10-16_1400.zip", name: "ClaudeBackup_2026-10-16_1400.zip", dir_tilde: "~/ClaudeBackups", size: 412_330_000, is_zip: true, mtime: now - 0.9 * day, when: "16.10.2026. 14:00", kind: "automatski" },
  { path: "/Users/petar/Desktop/ClaudeBackup_2026-10-12_2231.zip", name: "ClaudeBackup_2026-10-12_2231.zip", dir_tilde: "~/Desktop", size: 398_100_000, is_zip: true, mtime: now - 4.6 * day, when: "12.10.2026. 22:31", kind: "ručni" },
  { path: "/Users/petar/Projects/pos-cafe/claude-istorija/ClaudeBackup_pos-cafe_2026-10-09_1012", name: "ClaudeBackup_pos-cafe_2026-10-09_1012", dir_tilde: "~/Projects/pos-cafe/claude-istorija", size: 0, is_zip: false, mtime: now - 8 * day, when: "09.10.2026. 10:12", kind: "projekat" },
  { path: "/Users/petar/ClaudeBackups/ClaudeBackup_2026-10-15_1400.zip", name: "ClaudeBackup_2026-10-15_1400.zip", dir_tilde: "~/ClaudeBackups", size: 409_870_000, is_zip: true, mtime: now - 1.9 * day, when: "15.10.2026. 14:00", kind: "automatski" },
];

function overview(): Overview {
  return {
    cats: [
      { id: "code_sessions", title: "Claude Code razgovori", default: true, found: true, desc: "Svi razgovori iz terminala i Code taba, po projektima — sa pod-agentima, rezultatima alata i Claude-ovom memorijom projekta. Originalni .jsonl format koji Claude čita (~/.claude/projects).", note: "137 razgovora · 448,2 MB", size: 470_000_000, count: 912 },
      { id: "tmp_sessions", title: "Slike i scratchpad iz razgovora", default: true, found: true, desc: "Slike koje si nalepio u razgovore i Claude-ovi radni fajlovi (scratchpad). Stoje u /private/tmp i macOS ih briše pri restartu — sačuvaj ih dok postoje.", note: "64 fajla · 38,1 MB", size: 39_950_000, count: 64 },
      { id: "global", title: "Podešavanja, kontekst i istorija", default: true, found: true, desc: "Globalni CLAUDE.md, settings.json, ~/.claude.json (projekti, MCP serveri), istorija kucanih poruka (history.jsonl), planovi, zadaci i checkpoint snimci za rewind.", note: "1.018 fajlova · 22,7 MB", size: 23_800_000, count: 1018 },
      { id: "extensions", title: "Skillovi, agenti, komande, pluginovi", default: true, found: true, desc: "~/.claude/skills, agents, commands, plugins, output-styles, hooks i rules.", note: "7 skillova · 3 agenta · 2 komande · 4,9 MB", size: 5_140_000, count: 88 },
      { id: "project_files", title: "Claude fajlovi u projektima", default: true, found: true, desc: "Iz foldera svakog projekta samo CLAUDE.md, CLAUDE.local.md, .mcp.json i .claude/ — bez koda.", note: "19 fajlova · 186,4 KB", size: 190_900, count: 19 },
      { id: "desktop_code", title: "Code tab — lista razgovora", default: true, found: true, desc: "Kartice koje Claude aplikacija prikazuje u Code tabu (naslovi, arhivirano). Sami razgovori su u stavci „Claude Code razgovori”.", note: "96 fajlova · 412,0 KB", size: 421_900, count: 96 },
      { id: "cowork", title: "Cowork sesije", default: true, found: true, desc: "Cowork razgovori i fajlovi koje je Claude napravio ili koje si ubacio u Cowork.", note: "212 fajlova · 31,5 MB", size: 33_030_000, count: 212 },
      { id: "desktop_config", title: "Podešavanja Claude aplikacije", default: true, found: true, desc: "claude_desktop_config.json (MCP serveri), config.json i instalirane ekstenzije.", note: "14 fajlova · 1,1 MB", size: 1_150_000, count: 14 },
      { id: "user_files", title: "Folderi ~/Claude i ~/Documents/Claude", default: false, found: false, desc: "Fajlovi koje Claude aplikacija čuva za tebe (artefakti, rezultati zakazanih zadataka). Uključeno je ako je manje od 500 MB; ako je veće, proveri da tu nemaš nešto svoje (npr. kod).", note: "nije pronađeno", size: 0, count: 0 },
    ],
    projects,
    retention: { cleanup, desktop: null, managed: null, effective: cleanup ?? 30, error: "", version: "2.1.262", desktop_kept: true, version_known: true },
    schedule,
    running: ["Claude"],
    home: "/Users/petar",
    dests: [
      { label: "Desktop", path: "/Users/petar/Desktop", path_tilde: "~/Desktop", free: 182_300_000_000, kind: "home" },
      { label: "Documents", path: "/Users/petar/Documents", path_tilde: "~/Documents", free: 182_300_000_000, kind: "home" },
      { label: "Downloads", path: "/Users/petar/Downloads", path_tilde: "~/Downloads", free: 182_300_000_000, kind: "home" },
      { label: "ClaudeBackups", path: "/Users/petar/ClaudeBackups", path_tilde: "~/ClaudeBackups", free: 182_300_000_000, kind: "home" },
      { label: "Arhiva", path: "/Volumes/Arhiva", path_tilde: "/Volumes/Arhiva", free: 1_402_000_000_000, kind: "volume" },
    ],
    cc_version: "2.1.262",
    convs: 137,
    projects_with_convs: projects.length,
    claude_size: 573_600_000,
    skills: "7 skillova · 3 agenta · 2 komande",
    problems: 0,
    app_path: "/Applications/Claude Backup.app/Contents/MacOS/claude-backup",
    app_in_applications: true,
    last_backup: backups[0],
    is_mac: true,
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let cancelled = false;

async function fakeJob(job: string, phases: [string, string][], data: unknown, ms = 2600) {
  await sleep(30);
  cancelled = false;
  const steps = 60;
  for (const [phase, label] of phases) {
    for (let i = 0; i <= steps; i++) {
      if (cancelled) {
        emit("job-done", { job, ok: false, data: null, error: "Prekinuto — nedovršeni backup je uklonjen. U Claude-u ništa nije promenjeno." });
        return;
      }
      emit("job-progress", {
        job,
        phase,
        frac: i / steps,
        line1: `${label} ${Math.round((i / steps) * 1284)} / 1.284 fajlova · ${Math.round((i / steps) * 512)} MB / 512 MB`,
        line2: `~/.claude/projects/-Users-petar-Projects-pos-cafe/${["a3f1c2d4", "77b0e9aa", "c0ffee12"][i % 3]}-4e1b-49a0-9f2c-5d7e8a1b2c3d.jsonl`,
      });
      await sleep(ms / phases.length / steps);
    }
  }
  emit("job-done", { job, ok: true, data, error: "" });
}

const verifyOk = { ok: 1284, total: 1284, bytes: 537_100_000, bad: [], missing: [], error: "", passed: true, created: "16.10.2026. u 14:00", hostname: "MacBook-Pro", claude_code_version: "2.1.262", scope: "full" };

export async function mockCall<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  await sleep(cmd === "overview" ? 450 : 60);
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "overview":
      return r(winify(overview()));
    case "estimate": {
      const cats = (args.cats as string[]) ?? [];
      const o = overview();
      let bytes = o.cats.filter((c) => cats.includes(c.id)).reduce((a, c) => a + c.size, 0);
      let files = o.cats.filter((c) => cats.includes(c.id)).reduce((a, c) => a + c.count, 0);
      const ps = args.projects as string[] | null;
      if (ps) {
        const share = projects.filter((p) => ps.includes(p.key)).reduce((a, p) => a + p.size, 0) / projects.reduce((a, p) => a + p.size, 0);
        bytes *= 0.15 + 0.85 * share;
        files = Math.round(files * (0.15 + 0.85 * share));
      }
      return r({ files, bytes, convs: ps ? projects.filter((p) => ps.includes(p.key)).reduce((a, p) => a + p.sessions, 0) : 137 });
    }
    case "check_dest":
      return r(null);
    case "list_backups":
      return r(winify(backups));
    case "project_for_folder":
      return r((args.path as string).includes("novi-sajt") ? null : projects[0].key);
    case "project_preview": {
      const p = projects.find((x) => x.key === args.key) ?? projects[0];
      return r({ key: p.key, name: p.name, cwd_tilde: p.cwd_tilde, dest_tilde: `${p.cwd_tilde}/claude-istorija`, sessions: p.sessions, sessions_size: p.size * 0.82, memory: 3, images: 12, per_session: 140, cards: p.sessions, project_files: 4, prompts: 311 });
    }
    case "start_backup":
    case "start_project_backup": {
      const project = cmd === "start_project_backup";
      const path = project ? "/Users/petar/Projects/pos-cafe/claude-istorija/ClaudeBackup_pos-cafe_2026-10-17_1512.zip" : "/Users/petar/Desktop/ClaudeBackup_2026-10-17_1512.zip";
      const result = {
        path, path_tilde: path.replace("/Users/petar", "~"), is_zip: true, size: 401_200_000, files: 1284, bytes: 537_100_000, convs: project ? 48 : 137, has_html: true, has_md: true,
        verify: verifyOk, failed: [], unstable: [], new_after: ["~/.claude/projects/-Users-petar-Projects-pos-cafe/e1f2.jsonl"], changed_after: [],
        per_cat: [
          { id: "code_sessions", title: "Claude Code razgovori", files: 912, bytes: 470_000_000 },
          { id: "tmp_sessions", title: "Slike i scratchpad iz razgovora", files: 64, bytes: 39_950_000 },
          { id: "global", title: "Podešavanja, kontekst i istorija", files: 194, bytes: 21_800_000 },
          { id: "cowork", title: "Cowork sesije", files: 114, bytes: 5_350_000 },
        ],
        problems: [], seconds: 14.2,
      };
      const lines = [
        ["ok", "✔ Sačuvano 1.284 fajla (512,2 MB) — svaki je proveren i identičan originalu (SHA-256)."],
        ["", `Backup: ${result.path_tilde}  (382,6 MB)`],
        ["", `Čitljivo: ${project ? "48 razgovora" : "137 razgovora"} u Pregled.html i Markdown`],
        ["head", "Sačuvano"],
        ...result.per_cat.map((c) => ["ok", `✔ ${c.title} — ${c.files} fajlova`]),
        ["warn", "⚠ Claude je radio tokom backupa: 0 menjano dok je čitano, 1 novih i 0 promenjenih posle kopiranja."],
      ];
      fakeJob("backup", [["copy", "Kopiram"], ["render", "Pravim pregled"], ["verify", "Proveravam"]], { result, lines }, 4200);
      return r(undefined);
    }
    case "cancel_job":
      cancelled = true;
      return r(undefined);
    case "start_verify":
      fakeJob("verify", [["verify", "Proveravam"]], verifyOk, 2200);
      return r(undefined);
    case "start_restore_plan":
      fakeJob("plan", [["verify", "Proveravam"], ["plan", "Upoređujem"]], {
        verify: verifyOk,
        retention: overview().retention,
        cats: [
          { id: "code_sessions", title: "Claude Code razgovori", add: 41, same: 868, diff: 3, noproj: 0, unsafe_: 0, add_bytes: 38_400_000 },
          { id: "tmp_sessions", title: "Slike i scratchpad iz razgovora", add: 64, same: 0, diff: 0, noproj: 0, unsafe_: 0, add_bytes: 39_950_000 },
          { id: "global", title: "Podešavanja, kontekst i istorija", add: 12, same: 1001, diff: 5, noproj: 0, unsafe_: 0, add_bytes: 1_200_000 },
          { id: "extensions", title: "Skillovi, agenti, komande, pluginovi", add: 0, same: 88, diff: 0, noproj: 0, unsafe_: 0, add_bytes: 0 },
          { id: "project_files", title: "Claude fajlovi u projektima", add: 2, same: 15, diff: 0, noproj: 2, unsafe_: 0, add_bytes: 8_100 },
        ],
      }, 2400);
      return r(undefined);
    case "start_restore":
      fakeJob("restore", [["restore", "Vraćam"]], { result: { added: 119, appeared: 0, failed: [] }, retention: args.setRet ? { ok: true, message: "sačuvano i provereno", backup: "~/.claude/settings.json.pre-claude-backup-20261017-151200", changes: ['"cleanupPeriodDays": (nije podešeno = 30) → 3650'] } : null }, 2000);
      return r(undefined);
    case "get_retention":
      return r(overview().retention);
    case "set_retention": {
      const old = cleanup;
      cleanup = args.days as number;
      return r({ ok: true, message: "sačuvano i provereno", backup: "~/.claude/settings.json.pre-claude-backup-20261017-151200", changes: [`"cleanupPeriodDays": ${old ?? "(nije podešeno = 30)"} → ${cleanup}`] });
    }
    case "schedule_get":
      return r(winify(schedule));
    case "schedule_set": {
      const a = args as { freq: string; hour: number; minute: number; dest: string; keep: number };
      schedule = { ...schedule, enabled: true, freq: a.freq, hour: a.hour, minute: a.minute, dest: a.dest, dest_tilde: a.dest.replace("/Users/petar", "~"), keep: a.keep };
      return r(schedule);
    }
    case "schedule_remove":
      schedule = { ...schedule, enabled: false, loaded: false };
      return r("Automatski backup je isključen. Postojeći backupi su ostali gde jesu.");
    case "schedule_run_now":
      return r(undefined);
    case "app_version":
      return r("1.0.0 · jezgro app-1.0");
    default:
      return r(undefined);
  }
}
