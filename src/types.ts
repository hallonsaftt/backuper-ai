// Oblici podataka koje šalje Rust strana (src-tauri/src/lib.rs i backup-core).

export interface Cat {
  id: string;
  title: string;
  desc: string;
  default: boolean;
  found: boolean;
  note: string;
  size: number;
  count: number;
}

export interface Proj {
  key: string;
  cwd: string;
  cwd_tilde: string;
  name: string;
  sessions: number;
  size: number;
  last: number;
  last_text: string;
  exists: boolean;
  has_files: boolean;
}

export interface Dest {
  label: string;
  path: string;
  path_tilde: string;
  free: number;
  kind: "home" | "volume" | string;
}

export interface BackupFile {
  path: string;
  name: string;
  dir_tilde: string;
  size: number;
  is_zip: boolean;
  mtime: number;
  when: string;
  kind: "ručni" | "automatski" | "projekat" | string;
}

export interface Retention {
  cleanup: number | null;
  desktop: number | null;
  managed: number | null;
  effective: number;
  error: string;
  version: string;
  desktop_kept: boolean;
  version_known: boolean;
}

export interface LastRun {
  time: string;
  ok: boolean;
  message: string;
  path: string;
}

export interface Sched {
  enabled: boolean;
  broken: boolean;
  freq: "daily" | "weekly" | "monthly" | string;
  hour: number;
  minute: number;
  dest: string;
  dest_tilde: string;
  keep: number;
  loaded: boolean;
  program: string;
  describe: string;
  last_run: LastRun | null;
  log: string;
}

export interface Overview {
  cats: Cat[];
  projects: Proj[];
  retention: Retention;
  schedule: Sched;
  running: string[];
  home: string;
  dests: Dest[];
  cc_version: string;
  convs: number;
  projects_with_convs: number;
  claude_size: number;
  skills: string;
  problems: number;
  app_path: string;
  app_in_applications: boolean;
  last_backup: BackupFile | null;
  is_mac: boolean;
  os?: string;
}

export interface Estimate {
  files: number;
  bytes: number;
  convs: number;
}

export interface VerifyResult {
  ok: number;
  total: number;
  bytes: number;
  bad: string[];
  missing: string[];
  error: string;
  passed: boolean;
  created: string;
  hostname: string;
  claude_code_version: string;
  scope: string;
}

export interface CatCount {
  id: string;
  title: string;
  files: number;
  bytes: number;
}

export interface BackupResult {
  path: string;
  path_tilde: string;
  is_zip: boolean;
  size: number;
  files: number;
  bytes: number;
  convs: number;
  has_html: boolean;
  has_md: boolean;
  verify: VerifyResult;
  failed: [string, string][];
  unstable: string[];
  new_after: string[];
  changed_after: string[];
  per_cat: CatCount[];
  problems: [string, string][];
  seconds: number;
}

export type Line = [kind: "ok" | "warn" | "err" | "head" | "", text: string];

export interface BackupDone {
  result: BackupResult;
  lines: Line[];
}

export interface ProjectPreview {
  key: string;
  name: string;
  cwd_tilde: string;
  dest_tilde: string;
  sessions: number;
  sessions_size: number;
  memory: number;
  images: number;
  per_session: number;
  cards: number;
  project_files: number;
  prompts: number;
}

export interface CatPlan {
  id: string;
  title: string;
  add: number;
  same: number;
  diff: number;
  noproj: number;
  unsafe_: number;
  add_bytes: number;
}

export interface PlanDone {
  verify: VerifyResult;
  cats: CatPlan[];
  retention?: Retention;
}

export interface RestoreDone {
  result: { added: number; appeared: number; failed: [string, string][] };
  retention: SetResult | null;
}

export interface SetResult {
  ok: boolean;
  message: string;
  backup: string;
  changes: string[];
}

export interface Progress {
  job: string;
  phase: string;
  frac: number;
  line1: string;
  line2: string;
}

export interface JobDone<T = unknown> {
  job: string;
  ok: boolean;
  data: T;
  error: string;
}

export type Page = "overview" | "backup" | "project" | "archive" | "schedule" | "retention" | "help";
