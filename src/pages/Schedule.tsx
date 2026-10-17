import { CalendarClock, KeyRound, Moon, Play, Power, PowerOff, Sun } from "lucide-react";
import { useState } from "react";
import { call } from "../api";
import { DestPicker } from "../components/DestPicker";
import { Callout, cx, KV, PageHead, rise, Section, Seg } from "../components/ui";
import { fmtWhen, freqText, pad2 } from "../lib/format";
import { L } from "../lib/i18n";
import { isMac, isWin, SEP, T } from "../lib/platform";
import type { Overview, Sched } from "../types";

// macOS keeps background programs out of these folders without permission (Windows has no such rule)
const PROTECTED = isMac ? ["/Desktop", "/Documents", "/Downloads"] : [];

export function Schedule({ ov, onChanged }: { ov: Overview; onChanged: () => void }) {
  const s0 = ov.schedule;
  const defDest = ov.dests.find((d) => d.label === "ClaudeBackups")?.path ?? `${ov.home}${SEP}ClaudeBackups`;
  const [st, setSt] = useState<Sched>(s0);
  const [freq, setFreq] = useState(s0.enabled ? s0.freq : "daily");
  const [hour, setHour] = useState(s0.enabled ? s0.hour : 14);
  const [minute, setMinute] = useState(s0.enabled ? s0.minute : 0);
  const [dest, setDest] = useState(s0.enabled && s0.dest ? s0.dest : defDest);
  const [keep, setKeep] = useState(s0.enabled ? s0.keep : 14);
  const [msg, setMsg] = useState<{ tone: "ok" | "err" | "info"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const dests = ov.dests.some((d) => d.label === "ClaudeBackups") ? ov.dests : [{ label: "ClaudeBackups", path: defDest, path_tilde: isWin ? defDest : "~/ClaudeBackups", free: 0, kind: "home" }, ...ov.dests];
  const inProtected = PROTECTED.some((p) => dest.startsWith(ov.home + p));
  const byApp = !st.enabled || st.program === ov.app_path;
  const dirty = !byApp || !st.enabled || st.freq !== freq || st.hour !== hour || st.minute !== minute || st.dest !== dest || st.keep !== keep;

  const save = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const n = await call<Sched>("schedule_set", { freq, hour, minute, dest, keep });
      setSt(n);
      setMsg({
        tone: "ok",
        text: L(
          `Uključeno: ${freqText(freq)} u ${pad2(hour)}:${pad2(minute)}. Ne moraš ništa da pališ — ${T.os} pokreće backup sam.`,
          `On: ${freqText(freq)} at ${pad2(hour)}:${pad2(minute)}. Nothing to start — ${T.os} runs the backup by itself.`,
        ),
      });
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
      setMsg({
        tone: "info",
        text: L(
          `Pokrenuto u pozadini. Za minut-dva stiže obaveštenje, a backup će biti u ${st.dest_tilde}.`,
          `Started in the background. A notification arrives in a minute or two, and the backup will be in ${st.dest_tilde}.`,
        ),
      });
    } catch (e) {
      setMsg({ tone: "err", text: String(e) });
    }
  };

  const lr = st.last_run;
  return (
    <div className="pb-10">
      <PageHead
        eyebrow={L("Automatski backup", "Automatic backup")}
        title={
          <>
            {L("Backup", "Backups")} <em className="display-wonk text-accent">{L("sam od sebe", "on their own")}</em>
          </>
        }
        lead={L(
          `${T.os} pokreće backup u zadato vreme — aplikacija ne mora da bude otvorena. Stare automatske backupe briše po pravilu ispod; ručne nikad.`,
          `${T.os} starts the backup at the set time — the app doesn't need to be open. Old automatic backups are deleted by the rule below; manual ones never.`,
        )}
      />

      {!ov.app_in_applications && ov.is_mac && (
        <div className="rise mb-6" style={rise(1)}>
          <Callout tone="warn" title={L("Prvo prevuci Claude Backup u Applications", "First drag Claude Backup into Applications")}>
            {L("Aplikacija sad radi iz", "The app is running from")} <span className="mono">{ov.app_path.split(".app")[0]}.app</span>.{" "}
            {L(
              "Zakazivanje mora da zna stalnu lokaciju — prevuci je u folder Applications (Programi), otvori je odande i vrati se ovde.",
              "Scheduling needs a permanent location — drag it into the Applications folder, open it from there and come back here.",
            )}
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
              <span className="display text-[24px] leading-tight font-[480]">{st.enabled ? (st.broken ? L("Pokvareno", "Broken") : L("Uključeno", "On")) : L("Isključeno", "Off")}</span>
              {st.enabled && <span className={cx("tag", st.loaded ? "tag-ok" : "tag-warn")}>{st.loaded ? L(`${T.os} ga je prihvatio`, `accepted by ${T.os}`) : L("nije učitano", "not loaded")}</span>}
            </div>
            <div className="mt-0.5 text-[13.5px] text-muted">{st.enabled ? st.describe : L("Backup se pravi samo kad ga ti pokreneš.", "Backups happen only when you start one.")}</div>
          </div>
          {st.enabled && (
            <div className="flex gap-2">
              <button className="btn btn-ghost" onClick={runNow} disabled={busy}>
                <Play size={15} /> {L("Pokreni probno", "Test run")}
              </button>
              <button className="btn btn-quiet" onClick={off} disabled={busy}>
                <Power size={15} /> {L("Isključi", "Turn off")}
              </button>
            </div>
          )}
        </div>
        {st.enabled && (
          <div className="grid grid-cols-2 gap-x-10 border-t border-line bg-card-2 px-7 py-3">
            <KV k={L("Poslednji put", "Last run")} v={lr ? `${lr.ok ? "✔" : "✖"} ${fmtWhen(lr.time)}` : L("još nije pokrenut", "hasn't run yet")} />
            <KV k={L("Rezultat", "Result")} v={lr ? lr.message : "—"} />
            <KV k={L("Dnevnik", "Log")} v={st.log} mono />
            <KV k={L("Folder", "Folder")} v={st.dest_tilde} mono />
          </div>
        )}
      </div>

      {st.enabled && !byApp && (
        <div className="rise mb-6" style={rise(1)}>
          <Callout
            tone="info"
            title={
              /python/i.test(st.program)
                ? L("Ovo zakazivanje trenutno pokreće Python skripta", "This schedule is currently run by the Python script")
                : L("Ovo zakazivanje pokreće drugi program", "This schedule is run by another program")
            }
          >
            <span className="mono">{st.program}</span>.{" "}
            {L(
              "Klikni „Prebaci na aplikaciju” dole — ista podešavanja, samo će backup praviti ova aplikacija. Skripta ostaje gde jeste i može i dalje ručno.",
              "Click “Switch to the app” below — same settings, only this app will make the backups. The script stays where it is and still works by hand.",
            )}
          </Callout>
        </div>
      )}

      {msg && (
        <div className="mb-6">
          <Callout tone={msg.tone}>{msg.text}</Callout>
        </div>
      )}

      <Section n="01" title={L("Kada", "When")} i={2}>
        <div className="card flex flex-wrap items-center gap-8 px-6 py-5">
          <div>
            <div className="eyebrow mb-2">{L("Koliko često", "How often")}</div>
            <Seg
              value={freq}
              onChange={setFreq}
              options={[
                ["daily", L("Svaki dan", "Every day")],
                ["weekly", L("Svake nedelje", "Every week")],
                ["monthly", L("Svakog meseca", "Every month")],
              ]}
            />
          </div>
          <div>
            <div className="eyebrow mb-2">{L("U koliko sati", "At what time")}</div>
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
            → {freqText(freq)} {L("u", "at")} <b className="num font-[600] text-ink">{pad2(hour)}:{pad2(minute)}</b>
          </div>
        </div>
      </Section>

      <Section n="02" title={L(`Šta ako ${T.computer} nije uključen?`, `What if your ${T.computer} isn't on?`)} i={3}>
        <div className="grid grid-cols-3 gap-3">
          <Case
            icon={<Sun size={19} />}
            title={L(`${T.Computer} radi`, `${T.Computer} is on`)}
            text={L(`Backup se pravi tačno u ${pad2(hour)}:${pad2(minute)}, u pozadini.`, `The backup runs at exactly ${pad2(hour)}:${pad2(minute)}, in the background.`)}
          />
          <Case
            icon={<Moon size={19} />}
            title={L(`${T.Computer} spava`, `${T.Computer} is asleep`)}
            text={L(`Čim ga probudiš — ${T.os} pokrene propušten termin.`, `As soon as you wake it — ${T.os} runs the missed slot.`)}
          />
          <Case
            icon={<Power size={19} />}
            title={L(`${T.Computer} je ugašen`, `${T.Computer} is off`)}
            text={L(
              "Čim ga upališ i prijaviš se, aplikacija vidi da je termin propušten i odmah napravi backup.",
              "As soon as you turn it on and sign in, the app sees the slot was missed and backs up right away.",
            )}
          />
        </div>
      </Section>

      <Section
        n="03"
        title={L("Gde", "Where")}
        hint={isWin ? L("preporuka: ClaudeBackups u tvom korisničkom folderu", "recommended: ClaudeBackups in your user folder") : L("preporuka: ~/ClaudeBackups", "recommended: ~/ClaudeBackups")}
        i={4}
      >
        <DestPicker dests={dests} value={dest} onChange={setDest} createOk hint={(d) => (PROTECTED.some((p) => d.path === ov.home + p) ? L("dozvola", "permission") : null)} />
        {inProtected && (
          <div className="mt-3">
            <Callout
              tone="warn"
              icon={<KeyRound size={17} />}
              title={L("Za ovaj folder treba dozvola", "This folder needs permission")}
              action={
                <button className="btn btn-ghost" onClick={() => call("open_fda_settings")}>
                  {L("Otvori podešavanja", "Open settings")}
                </button>
              }
            >
              {L(
                "macOS ne pušta pozadinske programe u Desktop, Documents i Downloads. Daj aplikaciji Full Disk Access (Privatnost i bezbednost → Pun pristup disku) ili izaberi ~/ClaudeBackups.",
                "macOS keeps background programs out of Desktop, Documents and Downloads. Give the app Full Disk Access (Privacy & Security → Full Disk Access) or choose ~/ClaudeBackups.",
              )}
            </Callout>
          </div>
        )}
      </Section>

      <Section n="04" title={L("Koliko automatskih backupa da čuvam", "How many automatic backups to keep")} i={5}>
        <div className="card flex items-center gap-6 px-6 py-5">
          <Seg
            value={keep}
            onChange={setKeep}
            options={[
              [7, "7"],
              [14, "14"],
              [30, "30"],
              [90, "90"],
              [0, L("Sve", "All")],
            ]}
          />
          <p className="text-[13px] text-muted">
            {keep === 0
              ? L("Ništa se ne briše — pazi na prostor na disku.", "Nothing is deleted — keep an eye on disk space.")
              : L(
                  `Kad nastane ${keep + 1}. automatski backup, najstariji automatski se briše. Ručne backupe i istorije projekata nikad ne diram.`,
                  `When automatic backup no. ${keep + 1} is made, the oldest automatic one is deleted. Manual backups and project histories are never touched.`,
                )}
          </p>
        </div>
      </Section>

      {isMac && <div className="rise mb-8" style={rise(6)}>
        <Callout
          tone="info"
          icon={<KeyRound size={17} />}
          title={L("Savet: Full Disk Access", "Tip: Full Disk Access")}
          action={
            <button className="btn btn-ghost" onClick={() => call("open_fda_settings")}>
              {L("Otvori", "Open")}
            </button>
          }
        >
          {L(
            "Ako su ti projekti na Desktopu ili u Documents, pozadinskom backupu treba dozvola da pročita njihove CLAUDE.md fajlove. Dodaj „Claude Backup” u Pun pristup disku.",
            "If your projects are on the Desktop or in Documents, the background backup needs permission to read their CLAUDE.md files. Add “Claude Backup” to Full Disk Access.",
          )}
        </Callout>
      </div>}

      <div className="flex justify-end gap-3">
        <button className="btn btn-primary btn-lg" onClick={save} disabled={busy || !dirty || (!ov.app_in_applications && ov.is_mac)}>
          <CalendarClock size={18} /> {!st.enabled ? L("Uključi automatski backup", "Turn on automatic backup") : byApp ? L("Sačuvaj izmene", "Save changes") : L("Prebaci na aplikaciju", "Switch to the app")}
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
