import { ShieldCheck, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useJob } from "../job";
import { Spinner } from "./ui";

const PHASE: Record<string, [string, string]> = {
  copy: ["Kopiram i proveravam", "Svaki fajl se čita, kopira i odmah proverava SHA-256 zbirom."],
  read: ["Čitam razgovore", "Pripremam čitljivu verziju — originali ostaju netaknuti."],
  render: ["Pravim čitljivu verziju", "Pregled.html i Markdown, da razgovore možeš da čitaš bez Claude-a."],
  verify: ["Proveravam backup", "Ponovo čitam svaki fajl iz gotovog backupa i upoređujem SHA-256."],
  rescan: ["Gledam da li se nešto menjalo", "Ako je Claude radio za to vreme, zabeležiću šta je novo."],
  plan: ["Upoređujem sa tvojim Mac-om", "Tražim šta fali — ništa još ne diram."],
  restore: ["Vraćam fajlove", "Samo dodajem ono što fali. Postojeće ne prepisujem."],
};

export function JobOverlay() {
  const job = useJob();
  const [t0] = useState(() => Date.now());
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((x) => x + 1), 1000);
    return () => clearInterval(t);
  }, []);
  const p = job.progress;
  const [phaseTitle, phaseSub] = p ? (PHASE[p.phase] ?? [job.title, ""]) : [job.title, job.sub ?? "Pripremam…"];
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
            {job.name === "restore" ? "Samo dodajem fajlove koji fale — postojeće ne diram." : "Claude fajlove samo čitam — ništa ne brišem i ne menjam."}
          </div>
          {job.cancellable && (
            <button className="btn btn-quiet" onClick={job.cancel} disabled={job.cancelling}>
              <X size={15} /> {job.cancelling ? "Prekidam…" : "Prekini"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
