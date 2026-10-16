import { ArrowRight, CalendarClock, ChevronRight, FolderGit2, Hourglass, PackagePlus, RefreshCw, Sparkles, TerminalSquare } from "lucide-react";
import { Callout, cx, rise, Seal } from "../components/ui";
import { ago, fmtWhen, human, nConvs, num, plural, years } from "../lib/format";
import type { Overview as OV, Page } from "../types";

export function Overview({ ov, go, openProject, refresh }: { ov: OV; go: (p: Page) => void; openProject: (key: string) => void; refresh: () => void }) {
  const today = new Date().toLocaleDateString("sr-Latn-RS", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  const s = ov.schedule;
  const r = ov.retention;
  const lb = ov.last_backup;
  const retLow = r.effective < 365;

  return (
    <div>
      <header className="rise mb-9 flex items-start justify-between" style={rise(0)}>
        <div>
          <div className="eyebrow mb-3 flex items-center gap-2">
            <span className="inline-block h-[7px] w-[7px] rotate-45 bg-accent" />
            Arhiva · {today}
          </div>
          <h1 className="display text-[46px] leading-[1.02] font-[420]">
            Tvoja Claude <em className="display-wonk text-accent">arhiva</em>
          </h1>
        </div>
        <button className="btn btn-quiet mt-1" onClick={refresh} title="Ponovo pregledaj">
          <RefreshCw size={14} /> Osveži
        </button>
      </header>

      {/* knjiga inventara */}
      <div className="rise mb-8 grid grid-cols-4 border-y border-line-2" style={rise(1)}>
        <Ledger n={num(ov.convs)} label={nConvs(ov.convs).replace(/^[\d.]+ /, "")} sub="Claude Code" />
        <Ledger n={num(ov.projects_with_convs)} label={plural(ov.projects_with_convs, "projekat", "projekta", "projekata").replace(/^[\d.]+ /, "")} sub="sa istorijom" />
        <Ledger n={human(ov.claude_size).split(" ")[0]} label={human(ov.claude_size).split(" ")[1]} sub="ukupno za backup" />
        <Ledger n={ov.cc_version || "—"} label="" sub="Claude Code verzija" small />
      </div>

      <div className="grid grid-cols-[1.5fr_1fr] gap-5">
        {/* glavna akcija */}
        <div className="card rise relative overflow-hidden p-7" style={rise(2)}>
          <div className="pointer-events-none absolute -right-10 -bottom-12 opacity-[0.08]">
            <Seal size={250} animate={false} />
          </div>
          <div className="eyebrow mb-3">Kompletan backup</div>
          <h2 className="display max-w-[420px] text-[27px] leading-[1.12] font-[460]">Sačuvaj sve razgovore, podešavanja i skillove — proverene bajt po bajt.</h2>
          <p className="mt-3 max-w-[440px] text-[13.5px] leading-relaxed text-muted">
            Claude Code, Cowork, memorija projekata, slike iz razgovora i MCP podešavanja. U originalnom Claude formatu, plus čitljiv HTML i Markdown.
          </p>
          <div className="mt-6 flex gap-2.5">
            <button className="btn btn-primary btn-lg" onClick={() => go("backup")}>
              <PackagePlus size={18} /> Napravi backup
            </button>
            <button className="btn btn-ghost btn-lg" onClick={() => go("project")}>
              <FolderGit2 size={17} /> Samo jedan projekat
            </button>
          </div>
          <div className="perforated mt-7 pt-4 text-[12.5px] text-muted">
            {lb ? (
              <button className="group flex w-full items-center gap-2 text-left" onClick={() => go("archive")}>
                <span className="h-[7px] w-[7px] rounded-full bg-ok" />
                Poslednji backup <b className="font-[600] text-ink">{ago(lb.mtime)}</b>
                <span className="mono truncate text-[11.5px] text-faint">· {lb.dir_tilde}</span>
                <ChevronRight size={14} className="ml-auto opacity-0 transition-opacity group-hover:opacity-100" />
              </button>
            ) : (
              <span className="flex items-center gap-2">
                <span className="h-[7px] w-[7px] rounded-full bg-warn" /> Još nema nijednog backupa.
              </span>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-5">
          <Tile
            i={3}
            icon={<CalendarClock size={18} />}
            title="Automatski backup"
            tone={s.enabled && !s.broken ? (s.last_run && !s.last_run.ok ? "warn" : "ok") : "off"}
            status={s.enabled ? (s.broken ? "pokvaren" : "uključen") : "isključen"}
            onClick={() => go("schedule")}
          >
            {s.enabled ? (
              <>
                {s.describe.split(" → ")[0]}
                {s.last_run && (
                  <div className={cx("mt-1 text-[12px]", s.last_run.ok ? "text-faint" : "text-warn")}>
                    Poslednji put: {s.last_run.ok ? "uspešno" : "nije uspeo"} · {fmtWhen(s.last_run.time)}
                  </div>
                )}
              </>
            ) : (
              "Pravi backup sam — dnevno, nedeljno ili mesečno. Ako je Mac bio ugašen, uradi ga čim ga upališ."
            )}
          </Tile>
          <Tile
            i={4}
            icon={<Hourglass size={18} />}
            title="Čuvanje istorije"
            tone={retLow ? "warn" : "ok"}
            status={r.managed != null ? `${years(r.effective)} · organizacija` : years(r.effective)}
            onClick={() => go("retention")}
          >
            {retLow ? (
              <>
                Claude Code <b className="font-[600] text-ink">briše razgovore starije od {r.effective} dana</b> — u svim projektima.
              </>
            ) : (
              <>Razgovori se čuvaju {years(r.effective)}. Claude ih neće obrisati.</>
            )}
          </Tile>
        </div>
      </div>

      {ov.running.length > 0 && (
        <div className="rise mt-5" style={rise(5)}>
          <Callout tone="info" icon={<Sparkles size={17} />} title="Claude je trenutno otvoren">
            To je u redu — backup samo čita. Ako Claude nešto upiše dok traje, to će biti zabeleženo u izveštaju.
          </Callout>
        </div>
      )}

      {/* projekti */}
      <section className="rise mt-9" style={rise(6)}>
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="display text-[21px] font-[480]">Nedavni projekti</h2>
          <button className="btn btn-quiet" onClick={() => go("project")}>
            Svi projekti <ArrowRight size={14} />
          </button>
        </div>
        <div className="card divide-y divide-line">
          {ov.projects.slice(0, 5).map((p) => (
            <button key={p.key} onClick={() => openProject(p.key)} className="group flex w-full items-center gap-4 px-5 py-3.5 text-left transition-colors first:rounded-t-[14px] last:rounded-b-[14px] hover:bg-card-2">
              <div className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-bg-2 text-muted">
                <TerminalSquare size={17} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate text-[14px] font-[600]">{p.name}</div>
                <div className="mono truncate text-[11.5px] text-faint">{p.cwd_tilde}</div>
              </div>
              <div className="num w-[120px] text-right text-[12.5px] text-muted">{nConvs(p.sessions)}</div>
              <div className="w-[110px] text-right text-[12.5px] text-faint">{ago(p.last)}</div>
              <ChevronRight size={16} className="text-faint transition-transform group-hover:translate-x-0.5" />
            </button>
          ))}
          {ov.projects.length === 0 && <div className="px-5 py-6 text-[13px] text-muted">Nema Claude Code projekata na ovom Mac-u.</div>}
        </div>
      </section>
    </div>
  );
}

function Ledger({ n, label, sub, small }: { n: string; label: string; sub: string; small?: boolean }) {
  return (
    <div className="border-l border-line px-5 py-4 first:border-0 first:pl-0">
      <div className="flex items-baseline gap-1.5">
        <span className={cx("display num leading-none font-[380]", small ? "text-[30px]" : "text-[42px]")}>{n}</span>
        {label && <span className="text-[14px] text-muted">{label}</span>}
      </div>
      <div className="eyebrow mt-2">{sub}</div>
    </div>
  );
}

function Tile({ i, icon, title, status, tone, children, onClick }: { i: number; icon: React.ReactNode; title: string; status: string; tone: "ok" | "warn" | "off"; children: React.ReactNode; onClick: () => void }) {
  return (
    <button onClick={onClick} className="card rise group flex-1 p-5 text-left transition-transform hover:-translate-y-0.5" style={rise(i)}>
      <div className="flex items-center gap-2.5">
        <span className="text-muted">{icon}</span>
        <span className="text-[14px] font-[600]">{title}</span>
        <span className={cx("tag ml-auto", tone === "ok" && "tag-ok", tone === "warn" && "tag-warn")}>{status}</span>
      </div>
      <div className="mt-2.5 text-[13px] leading-relaxed text-muted">{children}</div>
    </button>
  );
}
