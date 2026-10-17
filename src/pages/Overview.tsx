import { ArrowRight, CalendarClock, ChevronRight, FolderGit2, Hourglass, PackagePlus, RefreshCw, Sparkles, TerminalSquare } from "lucide-react";
import { Callout, cx, rise, Seal } from "../components/ui";
import { ago, fmtWhen, human, nConvs, nDays, num, plWord, todayLong, years } from "../lib/format";
import { L } from "../lib/i18n";
import type { Overview as OV, Page } from "../types";
import { T } from "../lib/platform";

export function Overview({ ov, go, openProject, refresh }: { ov: OV; go: (p: Page) => void; openProject: (key: string) => void; refresh: () => void }) {
  const today = todayLong();
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
            {L("Arhiva", "Archive")} · {today}
          </div>
          <h1 className="display text-[46px] leading-[1.02] font-[420]">
            {L("Tvoja Claude", "Your Claude")} <em className="display-wonk text-accent">{L("arhiva", "archive")}</em>
          </h1>
        </div>
        <button className="btn btn-quiet mt-1" onClick={refresh} title={L("Ponovo pregledaj", "Scan again")}>
          <RefreshCw size={14} /> {L("Osveži", "Refresh")}
        </button>
      </header>

      {/* knjiga inventara */}
      <div className="rise mb-8 grid grid-cols-4 border-y border-line-2" style={rise(1)}>
        <Ledger n={num(ov.convs)} label={plWord(ov.convs, ["razgovor", "razgovora", "razgovora"], ["conversation", "conversations"])} sub="Claude Code" />
        <Ledger n={num(ov.projects_with_convs)} label={plWord(ov.projects_with_convs, ["projekat", "projekta", "projekata"], ["project", "projects"])} sub={L("sa istorijom", "with history")} />
        <Ledger n={human(ov.claude_size).split(" ")[0]} label={human(ov.claude_size).split(" ")[1]} sub={L("ukupno za backup", "total to back up")} />
        <Ledger n={ov.cc_version || "—"} label="" sub={L("Claude Code verzija", "Claude Code version")} small />
      </div>

      <div className="grid grid-cols-[1.5fr_1fr] gap-5">
        {/* glavna akcija */}
        <div className="card rise relative overflow-hidden p-7" style={rise(2)}>
          <div className="pointer-events-none absolute -right-10 -bottom-12 opacity-[0.08]">
            <Seal size={250} animate={false} />
          </div>
          <div className="eyebrow mb-3">{L("Kompletan backup", "Full backup")}</div>
          <h2 className="display max-w-[420px] text-[27px] leading-[1.12] font-[460]">
            {L("Sačuvaj sve razgovore, podešavanja i skillove — proverene bajt po bajt.", "Save every conversation, setting and skill — checked byte for byte.")}
          </h2>
          <p className="mt-3 max-w-[440px] text-[13.5px] leading-relaxed text-muted">
            {L(
              "Claude Code, Cowork, memorija projekata, slike iz razgovora i MCP podešavanja. U originalnom Claude formatu, plus čitljiv HTML i Markdown.",
              "Claude Code, Cowork, project memory, images from conversations and MCP settings. In Claude's original format, plus readable HTML and Markdown.",
            )}
          </p>
          <div className="mt-6 flex gap-2.5">
            <button className="btn btn-primary btn-lg" onClick={() => go("backup")}>
              <PackagePlus size={18} /> {L("Napravi backup", "Back up now")}
            </button>
            <button className="btn btn-ghost btn-lg" onClick={() => go("project")}>
              <FolderGit2 size={17} /> {L("Samo jedan projekat", "Just one project")}
            </button>
          </div>
          <div className="perforated mt-7 pt-4 text-[12.5px] text-muted">
            {lb ? (
              <button className="group flex w-full items-center gap-2 text-left" onClick={() => go("archive")}>
                <span className="h-[7px] w-[7px] rounded-full bg-ok" />
                {L("Poslednji backup", "Last backup")} <b className="font-[600] text-ink">{ago(lb.mtime)}</b>
                <span className="mono truncate text-[11.5px] text-faint">· {lb.dir_tilde}</span>
                <ChevronRight size={14} className="ml-auto opacity-0 transition-opacity group-hover:opacity-100" />
              </button>
            ) : (
              <span className="flex items-center gap-2">
                <span className="h-[7px] w-[7px] rounded-full bg-warn" /> {L("Još nema nijednog backupa.", "No backups yet.")}
              </span>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-5">
          <Tile
            i={3}
            icon={<CalendarClock size={18} />}
            title={L("Automatski backup", "Automatic backup")}
            tone={s.enabled && !s.broken ? (s.last_run && !s.last_run.ok ? "warn" : "ok") : "off"}
            status={s.enabled ? (s.broken ? L("pokvaren", "broken") : L("uključen", "on")) : L("isključen", "off")}
            onClick={() => go("schedule")}
          >
            {s.enabled ? (
              <>
                {s.describe.split(" → ")[0]}
                {s.last_run && (
                  <div className={cx("mt-1 text-[12px]", s.last_run.ok ? "text-faint" : "text-warn")}>
                    {L("Poslednji put", "Last run")}: {s.last_run.ok ? L("uspešno", "succeeded") : L("nije uspeo", "failed")} · {fmtWhen(s.last_run.time)}
                  </div>
                )}
              </>
            ) : (
              L(
                `Pravi backup sam — dnevno, nedeljno ili mesečno. Ako je ${T.computer} bio ugašen, uradi ga čim ga upališ.`,
                `Backs up on its own — daily, weekly or monthly. If your ${T.computer} was off, it catches up as soon as you turn it on.`,
              )
            )}
          </Tile>
          <Tile
            i={4}
            icon={<Hourglass size={18} />}
            title={L("Čuvanje istorije", "History retention")}
            tone={retLow ? "warn" : "ok"}
            status={r.managed != null ? `${years(r.effective)} · ${L("organizacija", "organization")}` : years(r.effective)}
            onClick={() => go("retention")}
          >
            {retLow ? (
              <>
                Claude Code <b className="font-[600] text-ink">{L(`briše razgovore starije od ${r.effective} dana`, `deletes conversations older than ${nDays(r.effective)}`)}</b>
                {L(" — u svim projektima.", " — in every project.")}
              </>
            ) : (
              <>{L(`Razgovori se čuvaju ${years(r.effective)}. Claude ih neće obrisati.`, `Conversations are kept for ${years(r.effective)}. Claude won't delete them.`)}</>
            )}
          </Tile>
        </div>
      </div>

      {ov.running.length > 0 && (
        <div className="rise mt-5" style={rise(5)}>
          <Callout tone="info" icon={<Sparkles size={17} />} title={L("Claude je trenutno otvoren", "Claude is open right now")}>
            {L(
              "To je u redu — backup samo čita. Ako Claude nešto upiše dok traje, to će biti zabeleženo u izveštaju.",
              "That's fine — a backup only reads. If Claude writes something while it runs, the report will note it.",
            )}
          </Callout>
        </div>
      )}

      {/* projekti */}
      <section className="rise mt-9" style={rise(6)}>
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="display text-[21px] font-[480]">{L("Nedavni projekti", "Recent projects")}</h2>
          <button className="btn btn-quiet" onClick={() => go("project")}>
            {L("Svi projekti", "All projects")} <ArrowRight size={14} />
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
          {ov.projects.length === 0 && <div className="px-5 py-6 text-[13px] text-muted">{L("Nema Claude Code projekata", "No Claude Code projects")} {T.onThis}.</div>}
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
