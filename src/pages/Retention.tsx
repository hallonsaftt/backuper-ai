import { Building2, Check as CheckIcon, Monitor, Save, X } from "lucide-react";
import { useState } from "react";
import { call } from "../api";
import { Callout, cx, PageHead, rise, Section } from "../components/ui";
import { nDays, pluralWord, years } from "../lib/format";
import type { Overview, Retention as Ret, SetResult } from "../types";

const OPTIONS: [number, string, string][] = [
  [3650, "10 godina", "Preporučeno — praktično zauvek."],
  [365, "1 godina", "Godinu dana unazad."],
  [90, "90 dana", "Tri meseca."],
  [30, "30 dana", "Claude-ovo podrazumevano."],
];

export function Retention({ ov, onChanged }: { ov: Overview; onChanged: () => void }) {
  const [r, setR] = useState<Ret>(ov.retention);
  const [days, setDays] = useState<number>(r.cleanup && r.cleanup >= 3650 ? r.cleanup : 3650);
  const [custom, setCustom] = useState("");
  const [res, setRes] = useState<SetResult | null>(null);
  const [busy, setBusy] = useState(false);

  const value = custom ? Math.max(0, parseInt(custom, 10) || 0) : days;
  const pct = Math.min(1, Math.log10(r.effective) / Math.log10(3650));
  const tone = r.effective >= 365 ? "ok" : "warn";

  const apply = async () => {
    setBusy(true);
    try {
      const out = await call<SetResult>("set_retention", { days: value });
      setRes(out);
      setR(await call<Ret>("get_retention"));
      onChanged();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="pb-10">
      <PageHead
        eyebrow="Čuvanje istorije · cleanupPeriodDays"
        title={
          <>
            Koliko dugo Claude <em className="display-wonk text-accent">pamti</em>
          </>
        }
        lead="Claude Code pri svakom pokretanju briše razgovore starije od zadatog broja dana — u svim projektima, ne samo onim koje ne koristiš. Starost se računa od poslednje izmene razgovora."
      />

      <div className="card rise mb-8 grid grid-cols-[auto_1fr] items-center gap-10 px-8 py-7" style={rise(1)}>
        <div className="relative h-[150px] w-[150px]">
          <svg viewBox="0 0 120 120" className="h-full w-full -rotate-90">
            <circle cx="60" cy="60" r="52" fill="none" stroke="var(--c-line)" strokeWidth="9" />
            <circle cx="60" cy="60" r="52" fill="none" stroke={tone === "ok" ? "var(--c-ok)" : "var(--c-warn)"} strokeWidth="9" strokeLinecap="round" strokeDasharray={`${pct * 326.7} 400`} style={{ transition: "stroke-dasharray .8s cubic-bezier(.2,.8,.2,1)" }} />
          </svg>
          <div className="absolute inset-0 grid place-items-center text-center">
            <div>
              <div className="display num text-[40px] leading-none font-[400]">{r.effective >= 365 ? Math.round(r.effective / 365) : r.effective}</div>
              <div className="mt-1 text-[12px] text-muted">{r.effective >= 365 ? pluralWord(Math.round(r.effective / 365), "godina", "godine", "godina") : pluralWord(r.effective, "dan", "dana", "dana")}</div>
            </div>
          </div>
        </div>
        <div>
          <div className="eyebrow mb-2">Sada</div>
          <div className="display text-[24px] leading-snug font-[460]">
            {tone === "ok" ? (
              <>Razgovori se čuvaju {years(r.effective)}.</>
            ) : (
              <>
                Razgovori stariji od <span className="text-warn">{nDays(r.effective)}</span> se brišu.
              </>
            )}
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <span className="tag">settings.json: {r.cleanup == null ? "nije podešeno (30)" : r.cleanup}</span>
            {r.managed != null && <span className="tag tag-warn">organizacija: {r.managed}</span>}
            {r.desktop_kept && <span className="tag tag-ok">desktop/Cowork razgovori se čuvaju</span>}
            {r.version && <span className="tag">Claude Code {r.version}</span>}
          </div>
          {r.error && <div className="mt-3 text-[12.5px] text-err">{r.error}</div>}
        </div>
      </div>

      {r.managed != null && (
        <div className="mb-6">
          <Callout tone="warn" icon={<Building2 size={17} />} title={`Tvoja organizacija je zaključala ovu vrednost na ${nDays(r.managed)}`}>
            Ono što podesiš ovde nema efekta dok važi podešavanje organizacije (managed-settings.json). Zato su redovni backupi još važniji.
          </Callout>
        </div>
      )}

      <Section n="01" title="Šta tačno briše, a šta ne" i={2}>
        <div className="grid grid-cols-2 gap-3">
          <div className="card p-5">
            <div className="mb-2.5 flex items-center gap-2 text-[13.5px] font-[650] text-err">
              <X size={16} /> Briše posle {nDays(r.effective)}
            </div>
            <ul className="space-y-1.5 text-[13px] text-muted">
              {["razgovore (.jsonl) i pod-agente", "rezultate alata", "checkpoint snimke za rewind (file-history)", "planove, zadatke, todo", "nalepljene slike i paste-cache"].map((t) => (
                <li key={t} className="flex gap-2">
                  <span className="text-faint">—</span>
                  {t}
                </li>
              ))}
            </ul>
          </div>
          <div className="card p-5">
            <div className="mb-2.5 flex items-center gap-2 text-[13.5px] font-[650] text-ok">
              <CheckIcon size={16} /> Ne briše nikad
            </div>
            <ul className="space-y-1.5 text-[13px] text-muted">
              {["memoriju projekata (memory/)", "history.jsonl — tvoje kucane poruke", "CLAUDE.md fajlove", "skillove, agente, komande", "podešavanja"].map((t) => (
                <li key={t} className="flex gap-2">
                  <span className="text-faint">—</span>
                  {t}
                </li>
              ))}
            </ul>
          </div>
        </div>
        <div className="mt-3">
          <Callout tone="info" icon={<Monitor size={17} />}>
            Od Claude Code 2.1.248 razgovori pokrenuti ili nastavljeni u Claude aplikaciji (Code tab) i Cowork-u čuvaju se bez obzira na starost — osim ako je podešeno <span className="mono">desktopSessionCleanupPeriodDays</span>. Razgovori iz terminala i dalje podležu ovom pravilu.
          </Callout>
        </div>
      </Section>

      <Section n="02" title="Podesi" i={3}>
        <div className="grid grid-cols-4 gap-2.5">
          {OPTIONS.map(([d, t, sub]) => {
            const on = !custom && days === d;
            return (
              <button
                key={d}
                onClick={() => {
                  setDays(d);
                  setCustom("");
                }}
                className={cx("rounded-xl border p-4 text-left transition-all", on ? "border-accent bg-accent-soft shadow-[0_0_0_3px_var(--c-accent-soft)]" : "border-line bg-card hover:border-line-2")}
              >
                <div className="display text-[22px] font-[460]">{t}</div>
                <div className="mt-1 text-[12px] text-muted">{sub}</div>
                {r.cleanup === d && <div className="tag tag-ok mt-2.5">trenutno</div>}
              </button>
            );
          })}
        </div>
        <div className="mt-3 flex items-center gap-3">
          <span className="text-[13px] text-muted">ili tačno</span>
          <input className="field num w-[110px]" inputMode="numeric" placeholder="npr. 730" value={custom} onChange={(e) => setCustom(e.target.value.replace(/\D/g, ""))} />
          <span className="text-[13px] text-muted">dana</span>
        </div>
      </Section>

      {res && (
        <div className="mb-6">
          <Callout tone={res.ok ? "ok" : "err"} title={res.ok ? `Sačuvano: ${res.message}` : `Nije sačuvano: ${res.message}`}>
            {res.changes.map((c) => (
              <div key={c} className="mono text-[12px]">
                {c}
              </div>
            ))}
            {res.backup && <div className="mt-1 text-[12px]">Kopija starog fajla: <span className="mono">{res.backup}</span></div>}
          </Callout>
        </div>
      )}

      <div className="flex items-center justify-end gap-4">
        <span className="text-[12.5px] text-muted">Menja samo taj jedan ključ u ~/.claude/settings.json — pre izmene pravi kopiju.</span>
        <button className="btn btn-primary btn-lg" disabled={busy || value < 1 || value === r.cleanup} onClick={apply}>
          <Save size={17} /> Sačuvaj {value >= 1 ? `(${years(value)})` : ""}
        </button>
      </div>
    </div>
  );
}
