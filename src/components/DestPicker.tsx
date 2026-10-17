import { FolderOpen, FolderPlus, HardDrive, Monitor, Usb } from "lucide-react";
import { useEffect, useState } from "react";
import { call, pickFolder } from "../api";
import { human } from "../lib/format";
import { L } from "../lib/i18n";
import type { Dest } from "../types";
import { cx } from "./ui";

const ICON: Record<string, typeof Monitor> = { Desktop: Monitor, Documents: FolderOpen, Downloads: FolderOpen, ClaudeBackups: HardDrive };

export function DestPicker({ dests, value, onChange, hint, createOk }: { dests: Dest[]; value: string; onChange: (p: string) => void; hint?: (d: Dest) => string | null; createOk?: boolean }) {
  const [custom, setCustom] = useState<string | null>(() => (value && !dests.some((d) => d.path === value) ? value : null));
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    if (!value) return;
    call<string | null>("check_dest", { path: value }).then((e) => live && setErr(e)).catch(() => {});
    return () => {
      live = false;
    };
  }, [value]);

  const choose = async () => {
    const p = await pickFolder(L("Gde da sačuvam backup?", "Where should the backup go?"));
    if (p) {
      setCustom(p);
      onChange(p);
    }
  };

  const all: Dest[] = [...dests];
  if (custom && !dests.some((d) => d.path === custom)) {
    all.push({ label: custom.split("/").filter(Boolean).pop() ?? custom, path: custom, path_tilde: custom, free: 0, kind: "custom" });
  }

  return (
    <div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(176px,1fr))] gap-2.5">
        {all.map((d) => {
          const on = d.path === value;
          const Ic = d.kind === "volume" ? Usb : (ICON[d.label] ?? FolderOpen);
          const h = hint?.(d);
          return (
            <button
              key={d.path}
              type="button"
              onClick={() => onChange(d.path)}
              className={cx(
                "relative flex flex-col items-start gap-2 rounded-xl border p-3.5 text-left transition-all",
                on ? "border-accent bg-accent-soft shadow-[0_0_0_3px_var(--c-accent-soft)]" : "border-line bg-card hover:border-line-2",
              )}
            >
              <div className="flex w-full items-center justify-between">
                <Ic size={18} className={on ? "text-accent" : "text-muted"} />
                {d.kind === "volume" && <span className="tag">{L("disk", "drive")}</span>}
                {h && <span className="tag tag-warn">{h}</span>}
              </div>
              <div className="min-w-0">
                <div className="truncate text-[13.5px] font-[600]">{d.label}</div>
                <div className="mono truncate text-[11px] text-muted">{d.path_tilde}</div>
              </div>
              {d.free > 0 && <div className="num text-[11px] text-faint">{human(d.free)} {L("slobodno", "free")}</div>}
            </button>
          );
        })}
        <button type="button" onClick={choose} className="flex min-h-[112px] flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-line-2 text-[13px] text-muted transition-colors hover:border-ink hover:text-ink">
          <FolderPlus size={19} />
          {L("Drugi folder…", "Another folder…")}
        </button>
      </div>
      {err && (createOk && /^(Taj folder ne postoji|That folder doesn't exist)/.test(err) ? (
        <div className="mt-2 text-[12.5px] text-muted">{L("Folder još ne postoji — napraviću ga.", "The folder doesn't exist yet — it will be created.")}</div>
      ) : (
        <div className="mt-2 text-[12.5px] text-err">{err}</div>
      ))}
    </div>
  );
}
