// Bridge to the Rust side. Outside Tauri (e.g. `npm run dev` in a browser) it uses fake data from mock.ts.
import { L } from "./lib/i18n";
import type { JobDone, Progress } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  const { mockCall } = await import("./mock");
  return mockCall<T>(cmd, args);
}

type ProgressCb = (p: Progress) => void;
type DoneCb = (d: JobDone) => void;
const progressSubs = new Set<ProgressCb>();
const doneSubs = new Set<DoneCb>();
let wired: Promise<void> | null = null;

export function wireEvents(): Promise<void> {
  if (wired) return wired;
  wired = (async () => {
    if (inTauri) {
      const { listen } = await import("@tauri-apps/api/event");
      await listen<Progress>("job-progress", (e) => progressSubs.forEach((f) => f(e.payload)));
      await listen<JobDone>("job-done", (e) => doneSubs.forEach((f) => f(e.payload)));
    } else {
      const m = await import("./mock");
      m.setEmitter((ev, payload) => {
        if (ev === "job-progress") progressSubs.forEach((f) => f(payload as Progress));
        else doneSubs.forEach((f) => f(payload as JobDone));
      });
    }
  })();
  return wired;
}

export function onProgress(f: ProgressCb) {
  progressSubs.add(f);
  return () => progressSubs.delete(f);
}
export function onDone(f: DoneCb) {
  doneSubs.add(f);
  return () => doneSubs.delete(f);
}

export async function pickFolder(title: string): Promise<string | null> {
  if (inTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, multiple: false, title });
    return typeof r === "string" ? r : null;
  }
  return "/Users/petar/Projects/novi-sajt";
}

export async function pickZip(): Promise<string | null> {
  if (inTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({
      directory: false,
      multiple: false,
      title: L("Izaberi Claude backup (.zip)", "Choose a Claude backup (.zip)"),
      filters: [{ name: "Claude backup", extensions: ["zip"] }],
    });
    return typeof r === "string" ? r : null;
  }
  return "/Volumes/Arhiva/ClaudeBackup_2026-09-01_1400.zip";
}

export const reveal = (path: string) => call<void>("reveal", { path });
export const openPath = (path: string) => call<void>("open_path", { path });
