import { ArrowLeft, BookOpenText, FolderSearch } from "lucide-react";
import type { ReactNode } from "react";
import { openPath, reveal } from "../api";
import { human, nFiles, num } from "../lib/format";
import type { BackupDone } from "../types";
import { Lines, Seal, rise } from "./ui";
import { L } from "../lib/i18n";
import { SEP, T } from "../lib/platform";

export function BackupResultView({ done, title, onBack, extra }: { done: BackupDone; title: string; onBack: () => void; extra?: ReactNode }) {
  const r = done.result;
  const ok = r.verify.passed && r.failed.length === 0;
  const passed = r.verify.passed;
  return (
    <div className="mx-auto max-w-[860px]">
      <div className="card rise overflow-hidden" style={rise(0)}>
        <div className="relative grid grid-cols-[1fr_auto] gap-8 px-9 pt-9 pb-7">
          <div>
            <div className="eyebrow mb-3">{passed ? L("Gotovo · provereno", "Done · verified") : L("Gotovo · sa greškama", "Done · with errors")}</div>
            <h1 className="display text-[36px] leading-[1.08] font-[440]">
              {passed ? (
                <>
                  {title} <em className="display-wonk text-ok">{L("je na sigurnom.", "is safe.")}</em>
                </>
              ) : (
                <>{L("Backup je napravljen, ali provera nije prošla.", "The backup was made, but verification failed.")}</>
              )}
            </h1>
            <p className="mt-3 max-w-[480px] text-[14px] text-muted">
              {passed
                ? `${nFiles(r.files)} (${human(r.bytes)}) — ${L("svaki pročitan ponovo iz gotovog backupa i identičan originalu.", "each one read back from the finished backup and identical to the original.")}`
                : L("Ne oslanjaj se na ovaj backup. Pokušaj ponovo, a ako se ponavlja, pogledaj izveštaj u backupu.", "Don't rely on this backup. Try again; if it keeps happening, look at the report inside the backup.")}
            </p>
            <div className="mt-6 flex flex-wrap gap-2.5">
              <button className="btn btn-ink" onClick={() => reveal(r.path)}>
                <FolderSearch size={16} /> {L("Prikaži u", "Show in")} {T.inFm}
              </button>
              {!r.is_zip && r.has_html && (
                <button className="btn btn-ghost" onClick={() => openPath(`${r.path}${SEP}${r.overview || "Pregled.html"}`)}>
                  <BookOpenText size={16} /> {L("Otvori", "Open")} {r.overview || "Pregled.html"}
                </button>
              )}
              <button className="btn btn-quiet" onClick={onBack}>
                <ArrowLeft size={15} /> {L("Nazad", "Back")}
              </button>
            </div>
          </div>
          <div className="pt-1 pr-2">
            <Seal ok={ok || passed} label={`${num(r.verify.ok)} / ${num(r.verify.total)}`} />
          </div>
        </div>
        <div className="perforated mx-9" />
        <div className="grid grid-cols-3 gap-0 px-9 py-5">
          <Stat k="Backup" v={r.is_zip ? human(r.size) : "folder"} sub={r.is_zip ? L("ZIP fajl", "ZIP file") : L("običan folder", "plain folder")} />
          <Stat k={L("Razgovora", "Conversations")} v={num(r.convs)} sub={[r.has_html && (r.overview || "HTML"), r.has_md && "Markdown"].filter(Boolean).join(" + ") || L("samo original", "original only")} />
          <Stat k={L("Trajanje", "Took")} v={`${r.seconds < 10 ? L(r.seconds.toFixed(1).replace(".", ","), r.seconds.toFixed(1)) : Math.round(r.seconds)} s`} sub={L("kopiranje + provera", "copy + verification")} />
        </div>
        <div className="mono selectable border-t border-line bg-card-2 px-9 py-3 text-[11.5px] text-muted">{r.path_tilde}</div>
      </div>

      {extra}

      <div className="card rise mt-5 px-9 py-6" style={rise(2)}>
        <div className="eyebrow mb-2">{L("Izveštaj", "Report")}</div>
        <Lines lines={done.lines.slice(1).filter(([, t]) => !t.startsWith("Backup: "))} />
      </div>
    </div>
  );
}

function Stat({ k, v, sub }: { k: string; v: string; sub: string }) {
  return (
    <div className="border-l border-line pl-4 first:border-0 first:pl-0">
      <div className="eyebrow">{k}</div>
      <div className="display num mt-1 text-[26px] leading-none font-[420]">{v}</div>
      <div className="mt-1 text-[12px] text-faint">{sub}</div>
    </div>
  );
}
