// Jedan dugi posao u isto vreme (backup, provera, vraćanje). Napredak stiže kao događaj iz Rust-a.
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { call, onDone, onProgress, wireEvents } from "./api";
import type { JobDone, Progress } from "./types";

export interface JobOpts {
  name: string;
  title: string;
  sub?: string;
  cancellable?: boolean;
}

interface JobState extends JobOpts {
  active: boolean;
  progress: Progress | null;
  cancelling: boolean;
}

interface JobCtx extends JobState {
  run<T>(cmd: string, args: Record<string, unknown>, o: JobOpts): Promise<JobDone<T>>;
  cancel(): void;
}

const Ctx = createContext<JobCtx | null>(null);
const idle: JobState = { active: false, name: "", title: "", progress: null, cancelling: false };

export function JobProvider({ children }: { children: ReactNode }) {
  const [st, setSt] = useState<JobState>(idle);

  const run = useCallback(async <T,>(cmd: string, args: Record<string, unknown>, o: JobOpts): Promise<JobDone<T>> => {
    await wireEvents();
    setSt({ ...idle, ...o, active: true });
    const unP = onProgress((p) => {
      if (p.job === o.name) setSt((s) => ({ ...s, progress: p }));
    });
    let unD: () => void = () => {};
    const done = new Promise<JobDone<T>>((res) => {
      unD = onDone((d) => {
        if (d.job === o.name) res(d as JobDone<T>);
      });
    });
    try {
      await call(cmd, args);
      return await done;
    } catch (e) {
      return { job: o.name, ok: false, data: null as T, error: String(e) };
    } finally {
      unP();
      unD();
      setSt(idle);
    }
  }, []);

  const cancel = useCallback(() => {
    setSt((s) => ({ ...s, cancelling: true }));
    call("cancel_job").catch(() => {});
  }, []);

  const v = useMemo(() => ({ ...st, run, cancel }), [st, run, cancel]);
  return <Ctx.Provider value={v}>{children}</Ctx.Provider>;
}

export function useJob() {
  const c = useContext(Ctx);
  if (!c) throw new Error("JobProvider fali");
  return c;
}
