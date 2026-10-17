import { Building2, Check as CheckIcon, Monitor, Save, X } from "lucide-react";
import { useState } from "react";
import { call } from "../api";
import { Callout, cx, PageHead, rise, Section } from "../components/ui";
import { nDays, plWord, years } from "../lib/format";
import { isSr, L } from "../lib/i18n";
import { isWin } from "../lib/platform";
import type { Overview, Retention as Ret, SetResult } from "../types";

const OPTIONS: [number, () => string, () => string][] = [
  [3650, () => L("10 godina", "10 years"), () => L("Preporučeno — praktično zauvek.", "Recommended — practically forever.")],
  [365, () => L("1 godina", "1 year"), () => L("Godinu dana unazad.", "One year back.")],
  [90, () => L("90 dana", "90 days"), () => L("Tri meseca.", "Three months.")],
  [30, () => L("30 dana", "30 days"), () => L("Claude-ovo podrazumevano.", "Claude's default.")],
];
const SETTINGS = isWin ? "%USERPROFILE%\\.claude\\settings.json" : "~/.claude/settings.json";

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
        eyebrow={L("Čuvanje istorije · cleanupPeriodDays", "History retention · cleanupPeriodDays")}
        title={
          <>
            {L("Koliko dugo Claude", "How long Claude")} <em className="display-wonk text-accent">{L("pamti", "remembers")}</em>
          </>
        }
        lead={L(
          "Claude Code pri svakom pokretanju briše razgovore starije od zadatog broja dana — u svim projektima, ne samo onim koje ne koristiš. Starost se računa od poslednje izmene razgovora.",
          "Every time Claude Code starts, it deletes conversations older than the set number of days — in every project, not just the ones you don't use. Age counts from a conversation's last change.",
        )}
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
              <div className="mt-1 text-[12px] text-muted">{r.effective >= 365
                  ? plWord(Math.round(r.effective / 365), ["godina", "godine", "godina"], ["year", "years"])
                  : plWord(r.effective, ["dan", "dana", "dana"], ["day", "days"])}</div>
            </div>
          </div>
        </div>
        <div>
          <div className="eyebrow mb-2">{L("Sada", "Now")}</div>
          <div className="display text-[24px] leading-snug font-[460]">
            {tone === "ok" ? (
              <>{L(`Razgovori se čuvaju ${years(r.effective)}.`, `Conversations are kept for ${years(r.effective)}.`)}</>
            ) : (
              <>
                {L("Razgovori stariji od", "Conversations older than")} <span className="text-warn">{nDays(r.effective)}</span> {L("se brišu.", "are deleted.")}
              </>
            )}
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <span className="tag">settings.json: {r.cleanup == null ? L("nije podešeno (30)", "not set (30)") : r.cleanup}</span>
            {r.managed != null && <span className="tag tag-warn">{L("organizacija", "organization")}: {r.managed}</span>}
            {r.desktop_kept && <span className="tag tag-ok">{L("desktop/Cowork razgovori se čuvaju", "desktop/Cowork conversations are kept")}</span>}
            {r.version && <span className="tag">Claude Code {r.version}</span>}
          </div>
          {r.error && <div className="mt-3 text-[12.5px] text-err">{r.error}</div>}
        </div>
      </div>

      {r.managed != null && (
        <div className="mb-6">
          <Callout tone="warn" icon={<Building2 size={17} />} title={L(`Tvoja organizacija je zaključala ovu vrednost na ${nDays(r.managed)}`, `Your organization locked this value at ${nDays(r.managed)}`)}>
            {L(
              "Ono što podesiš ovde nema efekta dok važi podešavanje organizacije (managed-settings.json). Zato su redovni backupi još važniji.",
              "What you set here has no effect while the organization setting (managed-settings.json) applies. That makes regular backups even more important.",
            )}
          </Callout>
        </div>
      )}

      <Section n="01" title={L("Šta tačno briše, a šta ne", "What exactly gets deleted, and what doesn't")} i={2}>
        <div className="grid grid-cols-2 gap-3">
          <div className="card p-5">
            <div className="mb-2.5 flex items-center gap-2 text-[13.5px] font-[650] text-err">
              <X size={16} /> {L("Briše posle", "Deleted after")} {nDays(r.effective)}
            </div>
            <ul className="space-y-1.5 text-[13px] text-muted">
              {(isSr()
                ? ["razgovore (.jsonl) i pod-agente", "rezultate alata", "checkpoint snimke za rewind (file-history)", "planove, zadatke, todo", "nalepljene slike i paste-cache"]
                : ["conversations (.jsonl) and subagents", "tool results", "rewind checkpoints (file-history)", "plans, tasks, todos", "pasted images and paste-cache"]
              ).map((t) => (
                <li key={t} className="flex gap-2">
                  <span className="text-faint">—</span>
                  {t}
                </li>
              ))}
            </ul>
          </div>
          <div className="card p-5">
            <div className="mb-2.5 flex items-center gap-2 text-[13.5px] font-[650] text-ok">
              <CheckIcon size={16} /> {L("Ne briše nikad", "Never deleted")}
            </div>
            <ul className="space-y-1.5 text-[13px] text-muted">
              {(isSr()
                ? ["memoriju projekata (memory/)", "history.jsonl — tvoje kucane poruke", "CLAUDE.md fajlove", "skillove, agente, komande", "podešavanja"]
                : ["project memory (memory/)", "history.jsonl — your typed prompts", "CLAUDE.md files", "skills, agents, commands", "settings"]
              ).map((t) => (
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
            {L(
              "Od Claude Code 2.1.248 razgovori pokrenuti ili nastavljeni u Claude aplikaciji (Code tab) i Cowork-u čuvaju se bez obzira na starost — osim ako je podešeno",
              "Since Claude Code 2.1.248, conversations started or resumed in the Claude app (Code tab) and in Cowork are kept regardless of age — unless",
            )}{" "}
            <span className="mono">desktopSessionCleanupPeriodDays</span>
            {L(". Razgovori iz terminala i dalje podležu ovom pravilu.", " is set. Conversations from the terminal still follow this rule.")}
          </Callout>
        </div>
      </Section>

      <Section n="02" title={L("Podesi", "Set it")} i={3}>
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
                <div className="display text-[22px] font-[460]">{t()}</div>
                <div className="mt-1 text-[12px] text-muted">{sub()}</div>
                {r.cleanup === d && <div className="tag tag-ok mt-2.5">{L("trenutno", "current")}</div>}
              </button>
            );
          })}
        </div>
        <div className="mt-3 flex items-center gap-3">
          <span className="text-[13px] text-muted">{L("ili tačno", "or exactly")}</span>
          <input className="field num w-[110px]" inputMode="numeric" placeholder={L("npr. 730", "e.g. 730")} value={custom} onChange={(e) => setCustom(e.target.value.replace(/\D/g, ""))} />
          <span className="text-[13px] text-muted">{L("dana", "days")}</span>
        </div>
      </Section>

      {res && (
        <div className="mb-6">
          <Callout tone={res.ok ? "ok" : "err"} title={res.ok ? `${L("Sačuvano", "Saved")}: ${res.message}` : `${L("Nije sačuvano", "Not saved")}: ${res.message}`}>
            {res.changes.map((c) => (
              <div key={c} className="mono text-[12px]">
                {c}
              </div>
            ))}
            {res.backup && (
              <div className="mt-1 text-[12px]">
                {L("Kopija starog fajla", "Copy of the old file")}: <span className="mono">{res.backup}</span>
              </div>
            )}
          </Callout>
        </div>
      )}

      <div className="flex items-center justify-end gap-4">
        <span className="text-[12.5px] text-muted">
          {L(`Menja samo taj jedan ključ u ${SETTINGS} — pre izmene pravi kopiju.`, `Changes only that one key in ${SETTINGS} — and makes a copy first.`)}
        </span>
        <button className="btn btn-primary btn-lg" disabled={busy || value < 1 || value === r.cleanup} onClick={apply}>
          <Save size={17} /> {L("Sačuvaj", "Save")} {value >= 1 ? `(${years(value)})` : ""}
        </button>
      </div>
    </div>
  );
}
