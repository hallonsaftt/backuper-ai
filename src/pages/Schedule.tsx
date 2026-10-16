import { CalendarClock, KeyRound, Moon, Play, Power, PowerOff, Sun } from "lucide-react";
import { useState } from "react";
import { call } from "../api";
import { DestPicker } from "../components/DestPicker";
import { Callout, cx, KV, PageHead, rise, Section, Seg } from "../components/ui";
import { fmtWhen, FREQ_TEXT, pad2 } from "../lib/format";
import type { Overview, Sched } from "../types";

const PROTECTED = ["/Desktop", "/Documents", "/Downloads"];

export function Schedule({ ov, onChanged }: { ov: Overview; onChanged: () => void }) {
  const s0 = ov.schedule;
  const defDest = ov.dests.find((d) => d.label === "ClaudeBackups")?.path ?? `${ov.home}/ClaudeBackups`;
  const [st, setSt] = useState<Sched>(s0);
  const [freq, setFreq] = useState(s0.enabled ? s0.freq : "daily");
  const [hour, setHour] = useState(s0.enabled ? s0.hour : 14);
  const [minute, setMinute] = useState(s0.enabled ? s0.minute : 0);
  const [dest, setDest] = useState(s0.enabled && s0.dest ? s0.dest : defDest);
  const [keep, setKeep] = useState(s0.enabled ? s0.keep : 14);
  const [msg, setMsg] = useState<{ tone: "ok" | "err" | "info"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const dests = ov.dests.some((d) => d.label === "ClaudeBackups") ? ov.dests : [{ label: "ClaudeBackups", path: defDest, path_tilde: "~/ClaudeBackups", free: 0, kind: "home" }, ...ov.dests];
  const inProtected = PROTECTED.some((p) => dest.startsWith(ov.home + p));
  const dirty = !st.enabled || st.freq !== freq || st.hour !== hour || st.minute !== minute || st.dest !== dest || st.keep !== keep;

  const save = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const n = await call<Sched>("schedule_set", { freq, hour, minute, dest, keep });
      setSt(n);
      setMsg({ tone: "ok", text: `Uključeno: ${FREQ_TEXT[freq]} u ${pad2(hour)}:${pad2(minute)}. Ne moraš ništa da pališ — macOS pokreće backup sam.` });
      onChanged();
    } catch (e) {
      setMsg({ tone: "err", text: String(e) });
    } finally {
      setBusy(false);
    }
  };
  const off = async () => {
    setBusy(true);
    try {
      const m = await call<string>("schedule_remove");
      setSt(await call<Sched>("schedule_get"));
      setMsg({ tone: "info", text: m });
      onChanged();
    } catch (e) {
      setMsg({ tone: "err", text: String(e) });
    } finally {
      setBusy(false);
    }
  };
  const runNow = async () => {
    try {
      await call("schedule_run_now");
      setMsg({ tone: "info", text: "Pokrenuto u pozadini. Za minut-dva stiže obaveštenje, a backup će biti u " + st.dest_tilde + "." });
    } catch (e) {
      setMsg({ tone: "err", text: String(e) });
    }
  };

  const lr = st.last_run;
  return (
    <div className="pb-10">
      <PageHead
        eyebrow="Automatski backup"
        title={
          <>
            Backup <em className="display-wonk text-accent">sam od sebe</em>
          </>
        }
        lead="macOS pokreće backup u zadato vreme — aplikacija ne mora da bude otvorena. Stare automatske backupe briše po pravilu ispod; ručne nikad."
      />

      {!ov.app_in_applications && ov.is_mac && (
        <div className="rise mb-6" style={rise(1)}>
          <Callout tone="warn" title="Prvo prevuci Claude Backup u Applications">
            Aplikacija sad radi iz <span className="mono">{ov.app_path.split(".app")[0]}.app</span>. Zakazivanje mora da zna stalnu lokaciju — prevuci je u folder Applications (Programi), otvori je odande i vrati se ovde.
          </Callout>
        </div>
      )}

      {/* stanje */}
      <div className="card rise mb-8 overflow-hidden" style={rise(1)}>
        <div className="flex items-center gap-5 px-7 py-6">
          <div className={cx("grid h-14 w-14 shrink-0 place-items-center rounded-2xl", st.enabled ? "bg-ok-soft text-ok" : "bg-bg-2 text-faint")}>
            {st.enabled ? <CalendarClock size={26} /> : <PowerOff size={24} />}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2.5">
              <span className="display text-[24px] leading-tight font-[480]">{st.enabled ? (st.broken ? "Pokvareno" : "Uključeno") : "Isključeno"}</span>
              {st.enabled && <span className={cx("tag", st.loaded ? "tag-ok" : "tag-warn")}>{st.loaded ? "macOS ga je prihvatio" : "nije učitano"}</span>}
            </div>
            <div className="mt-0.5 text-[13.5px] text-muted">{st.enabled ? st.describe : "Backup se pravi samo kad ga ti pokreneš."}</div>
          </div>
          {st.enabled && (
            <div className="flex gap-2">
              <button className="btn btn-ghost" onClick={runNow} disabled={busy}>
                <Play size={15} /> Pokreni probno
              </button>
              <button className="btn btn-quiet" onClick={off} disabled={busy}>
                <Power size={15} /> Isključi
              </button>
            </div>
          )}
        </div>
        {st.enabled && (
          <div className="grid grid-cols-2 gap-x-10 border-t border-line bg-card-2 px-7 py-3">
            <KV k="Poslednji put" v={lr ? `${lr.ok ? "✔" : "✖"} ${fmtWhen(lr.time)}` : "još nije pokrenut"} />
            <KV k="Rezultat" v={lr ? lr.message : "—"} />
            <KV k="Dnevnik" v={st.log} mono />
            <KV k="Folder" v={st.dest_tilde} mono />
          </div>
        )}
      </div>

      {msg && (
        <div className="mb-6">
          <Callout tone={msg.tone}>{msg.text}</Callout>
        </div>
      )}

      <Section n="01" title="Kada" i={2}>
        <div className="card flex flex-wrap items-center gap-8 px-6 py-5">
          <div>
            <div className="eyebrow mb-2">Koliko često</div>
            <Seg
              value={freq}
              onChange={setFreq}
              options={[
                ["daily", "Svaki dan"],
                ["weekly", "Svake nedelje"],
                ["monthly", "Svakog meseca"],
              ]}
            />
          </div>
          <div>
            <div className="eyebrow mb-2">U koliko sati</div>
            <div className="flex items-center gap-1.5">
              <select className="field num w-[74px] text-[14px]" value={hour} onChange={(e) => setHour(+e.target.value)}>
                {Array.from({ length: 24 }, (_, h) => (
                  <option key={h} value={h}>
                    {pad2(h)}
                  </option>
                ))}
              </select>
              <span className="display text-[20px]">:</span>
              <select className="field num w-[74px] text-[14px]" value={minute} onChange={(e) => setMinute(+e.target.value)}>
                {Array.from(new Set([0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, minute])).sort((a, b) => a - b).map((m) => (
                  <option key={m} value={m}>
                    {pad2(m)}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <div className="text-[13px] text-muted">
            → {FREQ_TEXT[freq]} u <b className="num font-[600] text-ink">{pad2(hour)}:{pad2(minute)}</b>
          </div>
        </div>
      </Section>

      <Section n="02" title="Šta ako Mac nije uključen?" i={3}>
        <div className="grid grid-cols-3 gap-3">
          <Case icon={<Sun size={19} />} title="Mac radi" text={`Backup se pravi tačno u ${pad2(hour)}:${pad2(minute)}, u pozadini.`} />
          <Case icon={<Moon size={19} />} title="Mac spava" text="Čim ga probudiš — macOS pokrene propušten termin." />
          <Case icon={<Power size={19} />} title="Mac je ugašen" text="Čim ga upališ i prijaviš se, aplikacija vidi da je termin propušten i odmah napravi backup." />
        </div>
      </Section>

      <Section n="03" title="Gde" hint="preporuka: ~/ClaudeBackups" i={4}>
        <DestPicker dests={dests} value={dest} onChange={setDest} hint={(d) => (PROTECTED.some((p) => d.path === ov.home + p) ? "dozvola" : null)} />
        {inProtected && (
          <div className="mt-3">
            <Callout tone="warn" icon={<KeyRound size={17} />} title="Za ovaj folder treba dozvola" action={<button className="btn btn-ghost" onClick={() => call("open_fda_settings")}>Otvori podešavanja</button>}>
              macOS ne pušta pozadinske programe u Desktop, Documents i Downloads. Daj aplikaciji Full Disk Access (Privatnost i bezbednost → Pun pristup disku) ili izaberi ~/ClaudeBackups.
            </Callout>
          </div>
        )}
      </Section>

      <Section n="04" title="Koliko automatskih backupa da čuvam" i={5}>
        <div className="card flex items-center gap-6 px-6 py-5">
          <Seg
            value={keep}
            onChange={setKeep}
            options={[
              [7, "7"],
              [14, "14"],
              [30, "30"],
              [90, "90"],
              [0, "Sve"],
            ]}
          />
          <p className="text-[13px] text-muted">
            {keep === 0 ? "Ništa se ne briše — pazi na prostor na disku." : `Kad nastane ${keep + 1}. automatski backup, najstariji automatski se briše. Ručne backupe i istorije projekata nikad ne diram.`}
          </p>
        </div>
      </Section>

      <div className="rise mb-8" style={rise(6)}>
        <Callout tone="info" icon={<KeyRound size={17} />} title="Savet: Full Disk Access" action={<button className="btn btn-ghost" onClick={() => call("open_fda_settings")}>Otvori</button>}>
          Ako su ti projekti na Desktopu ili u Documents, pozadinskom backupu treba dozvola da pročita njihove CLAUDE.md fajlove. Dodaj „Claude Backup” u Pun pristup disku.
        </Callout>
      </div>

      <div className="flex justify-end gap-3">
        <button className="btn btn-primary btn-lg" onClick={save} disabled={busy || !dirty || (!ov.app_in_applications && ov.is_mac)}>
          <CalendarClock size={18} /> {st.enabled ? "Sačuvaj izmene" : "Uključi automatski backup"}
        </button>
      </div>
    </div>
  );
}

function Case({ icon, title, text }: { icon: React.ReactNode; title: string; text: string }) {
  return (
    <div className="card p-5">
      <div className="flex items-center gap-2.5 text-accent">
        {icon}
        <span className="text-[14px] font-[600] text-ink">{title}</span>
      </div>
      <p className="mt-2 text-[12.5px] leading-relaxed text-muted">{text}</p>
    </div>
  );
}
