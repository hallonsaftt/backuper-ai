import { Archive, CalendarClock, FolderGit2, Hourglass, LayoutGrid, LifeBuoy, PackagePlus, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";
import type { Overview, Page } from "../types";
import { cx } from "./ui";

interface Item {
  id: Page;
  label: string;
  icon: ReactNode;
  badge?: ReactNode;
}

export function Sidebar({ page, onNav, ov, locked, version }: { page: Page; onNav: (p: Page) => void; ov: Overview | null; locked: boolean; version: string }) {
  const sched = ov?.schedule;
  const ret = ov?.retention;
  const schedDot = sched ? (sched.enabled && !sched.broken ? (sched.last_run && !sched.last_run.ok ? "warn" : "ok") : "off") : null;
  const retWarn = ret ? ret.effective < 365 : false;

  const groups: [string, Item[]][] = [
    [
      "Arhiva",
      [
        { id: "overview", label: "Pregled", icon: <LayoutGrid size={17} /> },
        { id: "backup", label: "Novi backup", icon: <PackagePlus size={17} /> },
        { id: "project", label: "Istorija projekta", icon: <FolderGit2 size={17} /> },
        { id: "archive", label: "Backupi i vraćanje", icon: <Archive size={17} /> },
      ],
    ],
    [
      "Zaštita",
      [
        {
          id: "schedule",
          label: "Automatski backup",
          icon: <CalendarClock size={17} />,
          badge: schedDot && <Dot tone={schedDot} />,
        },
        {
          id: "retention",
          label: "Čuvanje istorije",
          icon: <Hourglass size={17} />,
          badge: ret && (retWarn ? <span className="mono rounded bg-warn/20 px-1.5 py-[1px] text-[10px] text-warn">{ret.effective}d</span> : <Dot tone="ok" />),
        },
      ],
    ],
    ["", [{ id: "help", label: "Pomoć", icon: <LifeBuoy size={17} /> }]],
  ];

  return (
    <aside className="relative flex w-[244px] shrink-0 flex-col bg-side text-side-text">
      <div data-tauri-drag-region className="h-[46px] shrink-0" />
      <div data-tauri-drag-region className="flex items-center gap-3 px-5 pb-6">
        <Mark />
        <div data-tauri-drag-region>
          <div className="display text-[19px] leading-none font-[520] tracking-tight">Claude Backup</div>
          <div className="mt-1.5 font-mono text-[9.5px] tracking-[0.18em] text-side-muted uppercase">arhiva razgovora</div>
        </div>
      </div>

      <nav className="flex-1 space-y-5 px-3">
        {groups.map(([g, items], gi) => (
          <div key={gi}>
            {g && <div className="mb-1.5 px-3 font-mono text-[9.5px] tracking-[0.2em] text-side-muted/80 uppercase">{g}</div>}
            <div className="space-y-0.5">
              {items.map((it) => {
                const on = page === it.id;
                return (
                  <button
                    key={it.id}
                    disabled={locked}
                    onClick={() => onNav(it.id)}
                    className={cx(
                      "group relative flex h-[36px] w-full items-center gap-3 rounded-[9px] px-3 text-left text-[13.5px] transition-colors",
                      on ? "bg-side-2 text-white" : "text-side-text/75 hover:bg-white/[0.04] hover:text-side-text",
                      locked && "opacity-50",
                    )}
                  >
                    {on && <span className="absolute top-2 bottom-2 left-0 w-[3px] rounded-r bg-accent" />}
                    <span className={cx(on ? "text-accent" : "text-side-muted group-hover:text-side-text/80")}>{it.icon}</span>
                    <span className="flex-1 font-[520]">{it.label}</span>
                    {it.badge}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </nav>

      <div className="m-3 rounded-xl border border-white/[0.07] bg-white/[0.03] p-3.5">
        <div className="flex items-center gap-2 text-[12.5px] font-[600] text-side-text">
          <ShieldCheck size={15} className="text-[#8fbf88]" />
          Samo čita. Ništa ne briše.
        </div>
        <p className="mt-1 text-[11.5px] leading-snug text-side-muted">Svaki fajl se proverava SHA-256 zbirom. Login token se nikad ne kopira.</p>
        <div className="mt-2.5 font-mono text-[9.5px] tracking-wider text-side-muted/70">v{version}</div>
      </div>
    </aside>
  );
}

function Dot({ tone }: { tone: "ok" | "warn" | "off" }) {
  const c = { ok: "bg-[#8fbf88]", warn: "bg-[#d6a64a]", off: "bg-white/20" }[tone];
  return <span className={cx("h-[7px] w-[7px] rounded-full", c, tone === "ok" && "shadow-[0_0_0_3px_rgba(143,191,136,0.15)]")} />;
}

function Mark() {
  return (
    <svg width="34" height="34" viewBox="0 0 34 34" aria-hidden>
      <rect x="6" y="5" width="18" height="23" rx="3" fill="#8a7f6e" transform="rotate(8 15 16)" />
      <rect x="6" y="5" width="18" height="23" rx="3" fill="#f6f1e7" transform="rotate(-3 15 16)" />
      <rect x="9.5" y="10" width="10" height="2" rx="1" fill="#1c1a17" opacity=".45" />
      <rect x="9.5" y="14.5" width="7" height="2" rx="1" fill="#1c1a17" opacity=".25" />
      <circle cx="23.5" cy="24" r="7.5" fill="#c8613b" />
      <path d="M20.2 24.1l2.2 2.2 4.1-4.5" fill="none" stroke="#fbf6ee" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
