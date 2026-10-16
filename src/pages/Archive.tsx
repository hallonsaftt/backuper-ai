import { ArrowLeft, FileArchive, Folder, FolderOpen, FolderSearch, RotateCcw, ShieldCheck, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import { call, pickFolder, pickZip, reveal } from "../api";
import { Callout, Check, cx, KV, PageHead, rise, Seal, Spinner } from "../components/ui";
import { useJob } from "../job";
import { ago, fmtWhen, human, nFiles, num, years } from "../lib/format";
import type { BackupFile, CatPlan, Overview, PlanDone, RestoreDone, VerifyResult } from "../types";

type View =
  | { k: "list" }
  | { k: "verified"; file: string; vr: VerifyResult }
  | { k: "plan"; file: string; plan: PlanDone; cats: string[]; setRet: boolean }
  | { k: "restored"; file: string; res: RestoreDone };

const base = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

export function Archive({ ov, onChanged }: { ov: Overview; onChanged: () => void }) {
  const job = useJob();
  const [list, setList] = useState<BackupFile[] | null>(null);
  const [view, setView] = useState<View>({ k: "list" });
  const [error, setError] = useState<string | null>(null);

  const load = () => call<BackupFile[]>("list_backups").then(setList).catch(() => setList([]));
  useEffect(() => {
    load();
  }, []);

  const verify = async (file: string) => {
    setError(null);
    const d = await job.run<VerifyResult>("start_verify", { path: file }, { name: "verify", title: "Proveravam backup" });
    if (d.ok) setView({ k: "verified", file, vr: d.data });
    else setError(d.error);
  };

  const plan = async (file: string) => {
    setError(null);
    const d = await job.run<PlanDone>("start_restore_plan", { path: file }, { name: "plan", title: "Pripremam vraćanje" });
    if (!d.ok) return setError(d.error);
    if (!d.data.verify.passed) return setView({ k: "verified", file, vr: d.data.verify });
    const cats = d.data.cats.filter((c) => c.add > 0).map((c) => c.id);
    setView({ k: "plan", file, plan: d.data, cats, setRet: false });
  };

  const restore = async (v: Extract<View, { k: "plan" }>) => {
    const d = await job.run<RestoreDone>("start_restore", { cats: v.cats, setRet: v.setRet }, { name: "restore", title: "Vraćam iz backupa" });
    if (d.ok) {
      setView({ k: "restored", file: v.file, res: d.data });
      onChanged();
    } else setError(d.error);
  };

  const openOther = async (zip: boolean) => {
    const p = zip ? await pickZip() : await pickFolder("Izaberi folder backupa (ClaudeBackup_…)");
    if (p) return p;
    return null;
  };

  const back = () => {
    setView({ k: "list" });
    setError(null);
    load();
  };

  if (view.k === "verified") return <Verified file={view.file} vr={view.vr} onBack={back} onRestore={() => plan(view.file)} />;
  if (view.k === "plan") return <Plan v={view} ov={ov} set={(v) => setView(v)} onBack={back} onGo={() => restore(view)} error={error} />;
  if (view.k === "restored") return <Restored v={view} onBack={back} />;

  return (
    <div>
      <PageHead
        eyebrow="Backupi i vraćanje"
        title={
          <>
            Tvoji <em className="display-wonk text-accent">backupi</em>
          </>
        }
        lead="Proveri da li je backup ispravan ili vrati iz njega. Vraćanje samo dodaje fajlove koji fale — ništa postojeće ne prepisuje i ne briše."
        right={
          <div className="flex gap-2">
            <button className="btn btn-ghost" onClick={async () => { const p = await openOther(true); if (p) plan(p); }}>
              <FileArchive size={15} /> Otvori .zip…
            </button>
            <button className="btn btn-ghost" onClick={async () => { const p = await openOther(false); if (p) plan(p); }}>
              <FolderOpen size={15} /> Folder…
            </button>
          </div>
        }
      />

      {error && (
        <div className="mb-5">
          <Callout tone="err" title="Nije uspelo">
            {error}
          </Callout>
        </div>
      )}

      <div className="card rise overflow-hidden" style={rise(1)}>
        <div className="grid grid-cols-[minmax(0,1fr)_118px_84px_232px] gap-4 border-b border-line bg-card-2 px-6 py-2.5">
          <span className="eyebrow">Backup</span>
          <span className="eyebrow">Napravljen</span>
          <span className="eyebrow text-right">Veličina</span>
          <span />
        </div>
        {list === null && (
          <div className="flex items-center gap-2 px-6 py-8 text-[13px] text-muted">
            <Spinner size={14} /> Tražim backupe…
          </div>
        )}
        {list?.length === 0 && (
          <div className="px-6 py-10 text-center">
            <div className="display text-[20px]">Nema pronađenih backupa</div>
            <p className="mt-1 text-[13px] text-muted">Tražio sam na Desktopu, u Documents, Downloads, ~/ClaudeBackups, na diskovima i u projektima. Ako je negde drugde, otvori ga dugmetom gore.</p>
          </div>
        )}
        <div className="divide-y divide-line">
          {list?.map((b, i) => (
            <div key={b.path} className="rise group grid grid-cols-[minmax(0,1fr)_118px_84px_232px] items-center gap-4 px-6 py-3.5 hover:bg-card-2/60" style={rise(1.5 + i * 0.25)}>
              <div className="flex min-w-0 items-center gap-3.5">
                <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-bg-2 text-muted">{b.is_zip ? <FileArchive size={18} /> : <Folder size={18} />}</div>
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="mono truncate text-[12.5px] font-[600]" title={b.name}>{b.name.replace(/^ClaudeBackup_/, "").replace(/\.zip$/, "")}</span>
                    <span className={cx("tag shrink-0", b.kind === "automatski" && "tag-ok", b.kind === "projekat" && "tag-accent")}>{b.kind}</span>
                  </div>
                  <div className="mono truncate text-[11px] text-faint">{b.dir_tilde}</div>
                </div>
              </div>
              <div className="text-[12.5px]">
                <div>{ago(b.mtime)}</div>
                <div className="num text-[11px] text-faint">{b.when}</div>
              </div>
              <div className="num text-right text-[12.5px] text-muted">{b.is_zip ? human(b.size) : "folder"}</div>
              <div className="flex justify-end gap-1.5">
                <button className="btn btn-quiet" onClick={() => reveal(b.path)} title="Prikaži u Finderu">
                  <FolderSearch size={15} />
                </button>
                <button className="btn btn-ghost h-[32px] px-3 text-[12.5px]" onClick={() => verify(b.path)}>
                  <ShieldCheck size={14} /> Proveri
                </button>
                <button className="btn btn-ink h-[32px] px-3 text-[12.5px]" onClick={() => plan(b.path)}>
                  <Undo2 size={14} /> Vrati…
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Verified({ file, vr, onBack, onRestore }: { file: string; vr: VerifyResult; onBack: () => void; onRestore: () => void }) {
  const ok = vr.passed;
  return (
    <div className="mx-auto max-w-[820px]">
      <div className="card rise overflow-hidden">
        <div className="grid grid-cols-[1fr_auto] gap-8 px-9 pt-9 pb-7">
          <div>
            <div className="eyebrow mb-3">Provera backupa</div>
            <h1 className="display text-[34px] leading-[1.1] font-[440]">
              {ok ? (
                <>
                  Backup je <em className="display-wonk text-ok">ispravan.</em>
                </>
              ) : (
                <>
                  Backup <em className="display-wonk text-err">nije ispravan.</em>
                </>
              )}
            </h1>
            <p className="mt-3 text-[14px] text-muted">
              {ok
                ? `Svih ${num(vr.total)} fajlova (${human(vr.bytes)}) pročitano je iz backupa i SHA-256 zbir se poklapa sa onim zapisanim pri pravljenju.`
                : vr.error || `${num(vr.ok)} od ${num(vr.total)} fajlova je u redu.`}
            </p>
            <div className="mt-6 flex gap-2.5">
              {ok && (
                <button className="btn btn-ink" onClick={onRestore}>
                  <Undo2 size={16} /> Vrati iz ovog backupa…
                </button>
              )}
              <button className="btn btn-ghost" onClick={() => reveal(file)}>
                <FolderSearch size={16} /> Finder
              </button>
              <button className="btn btn-quiet" onClick={onBack}>
                <ArrowLeft size={15} /> Nazad
              </button>
            </div>
          </div>
          <Seal ok={ok} label={`${num(vr.ok)} / ${num(vr.total)}`} />
        </div>
        <div className="perforated mx-9" />
        <div className="grid grid-cols-2 gap-x-10 px-9 py-5">
          <KV k="Fajl" v={base(file)} mono />
          <KV k="Napravljen" v={fmtWhen(vr.created) || "—"} />
          <KV k="Mac" v={vr.hostname || "—"} />
          <KV k="Claude Code" v={vr.claude_code_version || "—"} />
        </div>
        {(vr.bad.length > 0 || vr.missing.length > 0) && (
          <div className="border-t border-line bg-err-soft px-9 py-4 text-[12.5px]">
            {vr.bad.slice(0, 8).map((b) => (
              <div key={b} className="mono text-err">
                ✖ izmenjen: {b}
              </div>
            ))}
            {vr.missing.slice(0, 8).map((b) => (
              <div key={b} className="mono text-err">
                ✖ fali: {b}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Plan({ v, ov, set, onBack, onGo, error }: { v: Extract<View, { k: "plan" }>; ov: Overview; set: (v: View) => void; onBack: () => void; onGo: () => void; error: string | null }) {
  const cats = v.plan.cats.filter((c) => c.add + c.same + c.diff + c.noproj + c.unsafe_ > 0);
  const chosen = cats.filter((c) => v.cats.includes(c.id));
  const nAdd = chosen.reduce((a, c) => a + c.add, 0);
  const bytes = chosen.reduce((a, c) => a + c.add_bytes, 0);
  const ret = v.plan.retention ?? ov.retention;
  const toggle = (c: CatPlan) => set({ ...v, cats: v.cats.includes(c.id) ? v.cats.filter((x) => x !== c.id) : [...v.cats, c.id] });

  return (
    <div className="pb-6">
      <button className="btn btn-quiet mb-4 -ml-2" onClick={onBack}>
        <ArrowLeft size={15} /> Backupi
      </button>
      <PageHead
        eyebrow="Vraćanje · korak 2 od 2"
        title={
          <>
            Šta <em className="display-wonk text-accent">fali</em> na ovom Mac-u
          </>
        }
        lead={
          <>
            Iz <span className="mono text-ink">{base(v.file)}</span> ({fmtWhen(v.plan.verify.created)}) — backup je proveren: {num(v.plan.verify.ok)}/{num(v.plan.verify.total)} ispravno.
          </>
        }
        right={<Seal ok size={104} label="" />}
      />

      <div className="card rise overflow-hidden" style={rise(1)}>
        <div className="grid grid-cols-[1fr_120px_100px_170px] gap-4 border-b border-line bg-card-2 px-6 py-2.5">
          <span className="eyebrow">Kategorija</span>
          <span className="eyebrow text-right text-accent">Dodaću</span>
          <span className="eyebrow text-right">Već imaš</span>
          <span className="eyebrow text-right">Drugačije (ne diram)</span>
        </div>
        <div className="divide-y divide-line">
          {cats.map((c) => {
            const can = c.add > 0;
            const on = v.cats.includes(c.id);
            return (
              <button key={c.id} disabled={!can} onClick={() => toggle(c)} className={cx("grid w-full grid-cols-[1fr_120px_100px_170px] items-center gap-4 px-6 py-3.5 text-left", can ? "hover:bg-card-2/60" : "opacity-60")}>
                <span className="flex items-center gap-3.5">
                  <Check on={can && on} />
                  <span className="text-[13.5px] font-[580]">{c.title}</span>
                </span>
                <span className="num text-right">
                  <span className={cx("text-[14px] font-[650]", can ? "text-accent" : "text-faint")}>{num(c.add)}</span>
                  {c.add_bytes > 0 && <span className="ml-1.5 text-[11.5px] text-faint">{human(c.add_bytes)}</span>}
                </span>
                <span className="num text-right text-[13px] text-muted">{num(c.same)}</span>
                <span className="num text-right text-[13px] text-muted">
                  {num(c.diff)}
                  {c.noproj > 0 && <span className="block text-[11px] text-faint">+{num(c.noproj)} bez foldera projekta</span>}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      <div className="rise mt-5 space-y-3" style={rise(2)}>
        <Callout tone="ok" icon={<ShieldCheck size={17} />} title="Vraćanje je bezbedno">
          Dodajem samo fajlove koji ne postoje. Fajlove koje već imaš — i one koji su se u međuvremenu promenili — ne diram. Login token se ne vraća; posle prelaska na novi Mac samo se ponovo prijaviš u Claude.
        </Callout>
        {ov.running.length > 0 && (
          <Callout tone="warn" title="Claude je otvoren">
            Preporuka: zatvori Claude aplikaciju i Claude Code pre vraćanja, pa ih otvori posle — tako sigurno vide vraćene razgovore.
          </Callout>
        )}
        {ret.effective < 3650 && ret.managed == null && (
          <button className="card flex w-full items-center gap-3.5 px-5 py-4 text-left" onClick={() => set({ ...v, setRet: !v.setRet })}>
            <Check on={v.setRet} />
            <div className="flex-1">
              <div className="text-[13.5px] font-[600]">Posle vraćanja podesi čuvanje istorije na 10 godina</div>
              <div className="text-[12.5px] text-muted">
                Sada je {years(ret.effective)} — Claude Code bi pri sledećem pokretanju obrisao starije vraćene razgovore.
              </div>
            </div>
          </button>
        )}
        {error && (
          <Callout tone="err" title="Nije uspelo">
            {error}
          </Callout>
        )}
      </div>

      <div className="rise mt-6 flex items-center justify-end gap-4" style={rise(3)}>
        <span className="num text-[13px] text-muted">
          {nFiles(nAdd)} · {human(bytes)}
        </span>
        <button className="btn btn-primary btn-lg" disabled={nAdd === 0} onClick={onGo}>
          <RotateCcw size={17} /> Vrati izabrano
        </button>
      </div>
    </div>
  );
}

function Restored({ v, onBack }: { v: Extract<View, { k: "restored" }>; onBack: () => void }) {
  const r = v.res.result;
  const ok = r.failed.length === 0;
  return (
    <div className="mx-auto max-w-[820px]">
      <div className="card rise overflow-hidden">
        <div className="grid grid-cols-[1fr_auto] gap-8 px-9 pt-9 pb-7">
          <div>
            <div className="eyebrow mb-3">Vraćanje završeno</div>
            <h1 className="display text-[34px] leading-[1.1] font-[440]">
              {ok ? (
                <>
                  Vraćeno <em className="display-wonk text-ok">{nFiles(r.added)}.</em>
                </>
              ) : (
                <>Vraćeno {nFiles(r.added)}, ali ne sve.</>
              )}
            </h1>
            <p className="mt-3 text-[14px] text-muted">
              Svaki vraćen fajl je proveren SHA-256 zbirom i ima originalni datum izmene.
              {r.appeared > 0 && ` ${nFiles(r.appeared)} se u međuvremenu pojavilo na Mac-u — njih nisam dirao.`} Otvori Claude — razgovori su tu.
            </p>
            <button className="btn btn-ink mt-6" onClick={onBack}>
              <ArrowLeft size={15} /> Nazad na backupe
            </button>
          </div>
          <Seal ok={ok} label={num(r.added)} />
        </div>
        {v.res.retention && (
          <div className="border-t border-line px-9 py-4">
            <div className={cx("text-[13px] font-[600]", v.res.retention.ok ? "text-ok" : "text-err")}>Čuvanje istorije: {v.res.retention.message}</div>
            {v.res.retention.changes.map((c) => (
              <div key={c} className="mono mt-1 text-[12px] text-muted">
                {c}
              </div>
            ))}
          </div>
        )}
        {r.failed.length > 0 && (
          <div className="border-t border-line bg-err-soft px-9 py-4 text-[12.5px]">
            {r.failed.slice(0, 10).map(([p, w]) => (
              <div key={p} className="mono text-err">
                ✖ {p} — {w}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
