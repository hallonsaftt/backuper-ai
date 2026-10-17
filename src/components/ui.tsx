import { Check as CheckIcon, CircleCheck, CircleX, Info, Lock, TriangleAlert } from "lucide-react";
import { useId, type CSSProperties, type ReactNode } from "react";
import { L } from "../lib/i18n";
import type { Line } from "../types";

export const cx = (...a: (string | false | null | undefined)[]) => a.filter(Boolean).join(" ");

/** Postepeno pojavljivanje: svaki sledeći element malo kasnije. */
export const rise = (i: number): CSSProperties => ({ animationDelay: `${60 + i * 55}ms` });

export function PageHead({ eyebrow, title, lead, right }: { eyebrow: string; title: ReactNode; lead?: ReactNode; right?: ReactNode }) {
  return (
    <header className="rise mb-8 flex items-end justify-between gap-6" style={rise(0)}>
      <div className="max-w-[640px]">
        <div className="eyebrow mb-3 flex items-center gap-2">
          <span className="inline-block h-[7px] w-[7px] rotate-45 bg-accent" />
          {eyebrow}
        </div>
        <h1 className="display text-[40px] leading-[1.05] font-[430] text-ink">{title}</h1>
        {lead && <p className="mt-3 text-[14.5px] leading-relaxed text-muted">{lead}</p>}
      </div>
      {right && <div className="shrink-0">{right}</div>}
    </header>
  );
}

export function Section({ n, title, hint, children, i = 1, right }: { n?: string; title: string; hint?: ReactNode; children: ReactNode; i?: number; right?: ReactNode }) {
  return (
    <section className="rise mb-9" style={rise(i)}>
      <div className="mb-3.5 flex items-baseline gap-3">
        {n && <span className="mono text-[12px] text-accent">{n}</span>}
        <h2 className="display text-[21px] font-[480]">{title}</h2>
        {hint && <span className="text-[12.5px] text-muted">{hint}</span>}
        {right && <div className="ml-auto">{right}</div>}
      </div>
      {children}
    </section>
  );
}

export function Check({ on, locked }: { on: boolean; locked?: boolean }) {
  return (
    <span className="box" data-on={locked ? "locked" : on ? "true" : "false"}>
      {locked ? <Lock size={11} strokeWidth={2.6} /> : on && <CheckIcon size={13} strokeWidth={3.2} />}
    </span>
  );
}

export function Switch({ on, onChange, disabled }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button type="button" className="switch" data-on={on} disabled={disabled} onClick={() => onChange(!on)} aria-pressed={on} />
  );
}

export function Seg<T extends string | number>({ value, onChange, options }: { value: T; onChange: (v: T) => void; options: [T, ReactNode][] }) {
  return (
    <div className="seg">
      {options.map(([v, label]) => (
        <button key={String(v)} type="button" data-on={v === value} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

export function Callout({ tone = "info", title, children, icon, action }: { tone?: "info" | "warn" | "err" | "ok"; title?: ReactNode; children?: ReactNode; icon?: ReactNode; action?: ReactNode }) {
  const colors = {
    info: "border-line-2 bg-card-2 text-ink",
    warn: "border-warn/35 bg-warn-soft",
    err: "border-err/35 bg-err-soft",
    ok: "border-ok/35 bg-ok-soft",
  }[tone];
  const ic = icon ?? { info: <Info size={17} />, warn: <TriangleAlert size={17} />, err: <CircleX size={17} />, ok: <CircleCheck size={17} /> }[tone];
  const icColor = { info: "text-muted", warn: "text-warn", err: "text-err", ok: "text-ok" }[tone];
  return (
    <div className={cx("flex items-start gap-3 rounded-xl border px-4 py-3.5", colors)}>
      <span className={cx("mt-[1px] shrink-0", icColor)}>{ic}</span>
      <div className="min-w-0 flex-1 text-[13.5px] leading-relaxed">
        {title && <div className="font-[600]">{title}</div>}
        {children && <div className={cx(title ? "mt-0.5 text-muted" : "")}>{children}</div>}
      </div>
      {action && <div className="shrink-0 self-center">{action}</div>}
    </div>
  );
}

/** Pečat: kružni tekst oko kvačice, kao žig u arhivi. */
export function Seal({ ok = true, label, size = 168, animate = true }: { ok?: boolean; label?: string; size?: number; animate?: boolean }) {
  const id = useId().replace(/:/g, "");
  const color = ok ? "var(--c-ok)" : "var(--c-err)";
  const ring = ok
    ? L("PROVERENO · SHA-256 · IDENTIČNO ORIGINALU · ", "VERIFIED · SHA-256 · IDENTICAL TO ORIGINAL · ")
    : L("PROVERA NIJE PROŠLA · NE OSLANJAJ SE · ", "VERIFICATION FAILED · DO NOT RELY ON IT · ");
  return (
    <div className={cx(animate && "stamp")} style={{ width: size, height: size, transform: animate ? undefined : "rotate(-11deg)" }}>
      <svg viewBox="0 0 200 200" width={size} height={size} style={{ color, overflow: "visible" }}>
        <defs>
          <filter id={`r${id}`}>
            <feTurbulence type="fractalNoise" baseFrequency="0.75" numOctaves="2" seed="7" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale="2.6" />
          </filter>
          <path id={`p${id}`} d="M100,100 m-75,0 a75,75 0 1,1 150,0 a75,75 0 1,1 -150,0" />
        </defs>
        <g filter={`url(#r${id})`} opacity="0.92">
          <circle cx="100" cy="100" r="94" fill="none" stroke="currentColor" strokeWidth="4" />
          <circle cx="100" cy="100" r="88" fill="none" stroke="currentColor" strokeWidth="1.2" />
          <circle cx="100" cy="100" r="61" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <text fill="currentColor" style={{ font: "600 12.5px var(--font-mono)", letterSpacing: "0.17em" }}>
            <textPath href={`#p${id}`} textLength="468" lengthAdjust="spacing">
              {ring}
            </textPath>
          </text>
          {ok ? (
            <path d="M74 101 l18 18 l36 -40" fill="none" stroke="currentColor" strokeWidth="9" strokeLinecap="round" strokeLinejoin="round" />
          ) : (
            <path d="M78 78 l44 44 M122 78 l-44 44" fill="none" stroke="currentColor" strokeWidth="9" strokeLinecap="round" />
          )}
          {label && (
            <text x="100" y="141" textAnchor="middle" fill="currentColor" style={{ font: "600 9px var(--font-mono)", letterSpacing: "0.04em" }}>
              {label}
            </text>
          )}
        </g>
      </svg>
    </div>
  );
}

export function Lines({ lines }: { lines: Line[] }) {
  return (
    <div className="space-y-1.5 text-[13.5px]">
      {lines.map(([k, t], i) =>
        k === "head" ? (
          <div key={i} className="eyebrow pt-3 pb-0.5">
            {t}
          </div>
        ) : (
          <div key={i} className={cx("flex gap-2 leading-relaxed", k === "ok" && "text-ink", k === "warn" && "text-warn", k === "err" && "text-err", k === "" && "text-muted")}>
            {k === "ok" && <CheckIcon size={15} className="mt-[3px] shrink-0 text-ok" strokeWidth={2.6} />}
            {k === "warn" && <TriangleAlert size={15} className="mt-[3px] shrink-0" />}
            {k === "err" && <CircleX size={15} className="mt-[3px] shrink-0" />}
            <span className={cx("selectable min-w-0 break-words", k === "" && "pl-[23px]")}>{t.replace(/^[✔✖⚠]\s*/, "")}</span>
          </div>
        ),
      )}
    </div>
  );
}

export function KV({ k, v, mono }: { k: ReactNode; v: ReactNode; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-dashed border-line py-2 last:border-0">
      <span className="text-[13px] text-muted">{k}</span>
      <span className={cx("min-w-0 truncate text-right text-[13px]", mono && "mono selectable")}>{v}</span>
    </div>
  );
}

export function Spinner({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" className="animate-spin" style={{ animationDuration: "0.9s" }}>
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="3" />
      <path d="M21 12a9 9 0 0 0-9-9" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}
