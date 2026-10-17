import { ShieldCheck, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useJob } from "../job";
import { Spinner } from "./ui";
import { L } from "../lib/i18n";
import { T } from "../lib/platform";

function phaseText(phase: string): [string, string] | null {
  switch (phase) {
    case "copy":
      return [L("Kopiram i proveravam", "Copying and checking"), L("Svaki fajl se čita, kopira i odmah proverava SHA-256 zbirom.", "Every file is read, copied and immediately checked with SHA-256.")];
    case "read":
      return [L("Čitam razgovore", "Reading conversations"), L("Pripremam čitljivu verziju — originali ostaju netaknuti.", "Preparing the readable version — the originals stay untouched.")];
    case "render":
      return [L("Pravim čitljivu verziju", "Making the readable version"), L("HTML pregled i Markdown, da razgovore možeš da čitaš bez Claude-a.", "An HTML overview and Markdown, so you can read conversations without Claude.")];
    case "verify":
      return [L("Proveravam backup", "Verifying the backup"), L("Ponovo čitam svaki fajl iz gotovog backupa i upoređujem SHA-256.", "Reading every file back from the finished backup and comparing SHA-256.")];
    case "rescan":
      return [L("Gledam da li se nešto menjalo", "Checking whether anything changed"), L("Ako je Claude radio za to vreme, zabeležiću šta je novo.", "If Claude was working meanwhile, I'll note what's new.")];
    case "plan":
      return [`${L("Upoređujem", "Comparing")} ${T.withYour}`, L("Tražim šta fali — ništa još ne diram.", "Looking for what's missing — nothing is touched yet.")];
    case "restore":
      return [L("Vraćam fajlove", "Restoring files"), L("Samo dodajem ono što fali. Postojeće ne prepisujem.", "Only adding what's missing. Nothing existing gets overwritten.")];
  }
  return null;
}

export function JobOverlay() {
  const job = useJob();
  const [t0] = useState(() => Date.now());
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((x) => x + 1), 1000);
    return () => clearInterval(t);
  }, []);
  const p = job.progress;
  const [phaseTitle, phaseSub] = p ? (phaseText(p.phase) ?? [job.title, ""]) : [job.title, job.sub ?? L("Pripremam…", "Getting ready…")];
  const pct = p ? Math.max(0, Math.min(1, p.frac)) : 0;
  const secs = Math.round((Date.now() - t0) / 1000);

  return (
    <div className="absolute inset-0 z-30 grid place-items-center bg-bg/80 backdrop-blur-[6px]">
      <div className="card rise w-[min(620px,88%)] overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-7 py-4">
          <div className="eyebrow flex items-center gap-2 text-accent">
            <Spinner size={13} /> {job.title}
          </div>
          <div className="mono num text-[12px] text-faint">
            {Math.floor(secs / 60)}:{String(secs % 60).padStart(2, "0")}
          </div>
        </div>
        <div className="px-7 pt-6 pb-7">
          <div className="flex items-end justify-between gap-6">
            <div className="min-w-0">
              <h2 className="display text-[28px] leading-tight font-[460]">{phaseTitle}</h2>
              <p className="mt-1.5 text-[13.5px] text-muted">{phaseSub}</p>
            </div>
            <div className="display num shrink-0 text-[44px] leading-none font-[380] text-accent">
              {Math.floor(pct * 100)}
              <span className="text-[22px] text-muted">%</span>
            </div>
          </div>
          <div className="mt-6">
            <div className="meter" key={p?.phase ?? "-"}>
              <i style={{ width: `${(pct * 100).toFixed(2)}%` }} />
            </div>
            <div className="ticks" aria-hidden>
              {Array.from({ length: 41 }, (_, i) => (
                <span key={i} />
              ))}
            </div>
          </div>
          <div className="mt-4 min-h-[42px] space-y-1">
            <div className="num text-[13px] text-ink">{p?.line1 || " "}</div>
            <div className="mono truncate text-[11.5px] text-faint" dir="rtl" style={{ textAlign: "left" }}>
              <bdi>{p?.line2 || " "}</bdi>
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between gap-4 bg-card-2 px-7 py-3.5">
          <div className="flex items-center gap-2 text-[12.5px] text-muted">
            <ShieldCheck size={15} className="text-ok" />
            {job.name === "restore"
              ? L("Samo dodajem fajlove koji fale — postojeće ne diram.", "Only adding missing files — existing ones stay untouched.")
              : L("Claude fajlove samo čitam — ništa ne brišem i ne menjam.", "Claude's files are only read — nothing is deleted or changed.")}
          </div>
          {job.cancellable && (
            <button className="btn btn-quiet" onClick={job.cancel} disabled={job.cancelling}>
              <X size={15} /> {job.cancelling ? L("Prekidam…", "Cancelling…") : L("Prekini", "Cancel")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
