import { FileArchive, Folder, FolderInput, FolderSearch, GitBranch, Search, TerminalSquare } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { call, pickFolder } from "../api";
import { BackupResultView } from "../components/ResultView";
import { Callout, cx, KV, PageHead, rise, Seg, Spinner, Switch } from "../components/ui";
import { useJob } from "../job";
import { ago, human, nConvs, num } from "../lib/format";
import type { BackupDone, Overview, ProjectPreview } from "../types";

export function Project({ ov, initial, onDone }: { ov: Overview; initial: string | null; onDone: () => void }) {
  const job = useJob();
  const eligible = useMemo(() => ov.projects.filter((p) => p.cwd && p.cwd !== ov.home && p.cwd !== "/"), [ov]);
  const ok = (k: string | null) => !!k && eligible.some((p) => p.key === k);
  const [sel, setSel] = useState<string | null>(ok(initial) ? initial : (eligible[0]?.key ?? null));
  const [q, setQ] = useState("");
  const [pv, setPv] = useState<ProjectPreview | null>(null);
  const [html, setHtml] = useState(true);
  const [md, setMd] = useState(true);
  const [zip, setZip] = useState(false);
  const [notFound, setNotFound] = useState<string | null>(null);
  const [done, setDone] = useState<BackupDone | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (ok(initial)) setSel(initial);
  }, [initial]);

  useEffect(() => {
    setPv(null);
    if (!sel) return;
    let live = true;
    call<ProjectPreview>("project_preview", { key: sel }).then((p) => live && setPv(p)).catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [sel]);

  const shown = useMemo(() => eligible.filter((p) => !q || (p.name + p.cwd_tilde).toLowerCase().includes(q.toLowerCase())), [eligible, q]);
  const proj = ov.projects.find((p) => p.key === sel);

  const fromFolder = async () => {
    setNotFound(null);
    const path = await pickFolder("Izaberi folder projekta");
    if (!path) return;
    const key = await call<string | null>("project_for_folder", { path });
    if (key) setSel(key);
    else setNotFound(path);
  };

  const start = async () => {
    if (!sel) return;
    setError(null);
    const d = await job.run<BackupDone>("start_project_backup", { key: sel, html, md, zip }, { name: "backup", title: `Čuvam istoriju — ${proj?.name ?? ""}`, cancellable: true });
    if (d.ok) {
      setDone(d.data);
      onDone();
    } else setError(d.error);
  };

  if (done)
    return (
      <BackupResultView
        done={done}
        title={`Istorija projekta ${proj?.name ?? ""}`}
        onBack={() => setDone(null)}
        extra={
          <div className="rise mt-5">
            <Callout tone="info" icon={<GitBranch size={17} />} title="Folder claude-istorija je van git-a">
              U njemu je .gitignore sa „*”, jer razgovori mogu da sadrže lozinke i API ključeve. Ako ipak želiš da ga deliš, prvo pregledaj sadržaj.
            </Callout>
          </div>
        }
      />
    );

  return (
    <div>
      <PageHead
        eyebrow="Istorija projekta"
        title={
          <>
            Sačuvaj istoriju <em className="display-wonk text-accent">u sam projekat</em>
          </>
        }
        lead="Izaberi projekat — sve razgovore, memoriju, slike i Claude fajlove tog projekta sačuvaću u njegov folder claude-istorija/. Ostali projekti se ne diraju."
      />

      <div className="rise grid grid-cols-[320px_minmax(0,1fr)] gap-5" style={rise(1)}>
        {/* spisak */}
        <div className="card flex max-h-[calc(100vh-250px)] min-h-[420px] flex-col overflow-hidden">
          <div className="space-y-2.5 border-b border-line p-3.5">
            <button className="btn btn-ink w-full" onClick={fromFolder}>
              <FolderInput size={16} /> Izaberi folder projekta…
            </button>
            <div className="relative">
              <Search size={14} className="absolute top-1/2 left-3 -translate-y-1/2 text-faint" />
              <input className="field h-[34px] w-full pl-8 text-[13px]" placeholder={`Traži među ${eligible.length} projekata…`} value={q} onChange={(e) => setQ(e.target.value)} />
            </div>
          </div>
          <div className="scroll flex-1 p-1.5">
            {shown.map((p) => {
              const on = p.key === sel;
              return (
                <button key={p.key} onClick={() => setSel(p.key)} className={cx("flex w-full items-center gap-3 rounded-[10px] px-3 py-2.5 text-left transition-colors", on ? "bg-ink text-bg" : "hover:bg-card-2")}>
                  <TerminalSquare size={16} className={on ? "text-accent" : "text-faint"} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13.5px] font-[600]">{p.name}</div>
                    <div className={cx("mono truncate text-[10.5px]", on ? "text-bg/60" : "text-faint")}>{p.cwd_tilde}</div>
                  </div>
                  <div className={cx("text-right text-[11px]", on ? "text-bg/70" : "text-muted")}>
                    <div className="num">{num(p.sessions)}</div>
                    <div className={on ? "text-bg/50" : "text-faint"}>{ago(p.last).replace("pre ", "")}</div>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* detalji */}
        <div className="space-y-4">
          {notFound && (
            <Callout tone="warn" title="Taj folder nije Claude Code projekat">
              <span className="mono">{notFound}</span> — za njega nema sačuvanih razgovora. Izaberi folder u kome si pokretao Claude.
            </Callout>
          )}
          {proj && (
            <div className="card overflow-hidden">
              <div className="px-7 pt-6 pb-5">
                <div className="eyebrow">Projekat</div>
                <h2 className="display mt-1.5 text-[30px] leading-tight font-[460]">{proj.name}</h2>
                <div className="mono selectable mt-1 text-[12px] text-muted">{proj.cwd_tilde}</div>
                <div className="num mt-3 text-[14px]">
                  <b className="font-[650]">{nConvs(proj.sessions)}</b>
                  {pv && <span className="text-muted"> · {human(pv.sessions_size)} · poslednji {ago(proj.last)}</span>}
                </div>
                {!proj.exists && (
                  <div className="mt-3">
                    <Callout tone="warn" title="Folder projekta više ne postoji">
                      Razgovori su i dalje u Claude-u, ali nemam gde da ih sačuvam u projekat. Napravi kompletan backup umesto toga.
                    </Callout>
                  </div>
                )}
              </div>
              <div className="grid grid-cols-2 gap-x-8 border-t border-line px-7 py-4">
                {pv ? (
                  <>
                    <KV k="Kucane poruke" v={num(pv.prompts)} />
                    <KV k="Slike, scratchpad" v={num(pv.images)} />
                    <KV k="Rewind, zadaci" v={num(pv.per_session)} />
                    <KV k="Code tab kartice" v={num(pv.cards)} />
                    <KV k="Memorija" v={num(pv.memory)} />
                    <KV k="Claude fajlovi" v={num(pv.project_files)} />
                  </>
                ) : (
                  <div className="col-span-2 flex items-center gap-2 py-3 text-[13px] text-muted">
                    <Spinner size={14} /> Brojim…
                  </div>
                )}
              </div>
              <div className="space-y-3 border-t border-line bg-card-2 px-7 py-4">
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">Pregled.html</span>
                  <Switch on={html} onChange={setHtml} />
                </div>
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">Markdown (.md)</span>
                  <Switch on={md} onChange={setMd} />
                </div>
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">Pakovanje</span>
                  <Seg
                    value={zip ? "zip" : "dir"}
                    onChange={(v) => setZip(v === "zip")}
                    options={[
                      ["dir", <span className="flex items-center gap-1.5"><Folder size={14} /> Folder</span>],
                      ["zip", <span className="flex items-center gap-1.5"><FileArchive size={14} /> ZIP</span>],
                    ]}
                  />
                </div>
              </div>
              <div className="flex items-center gap-4 border-t border-line px-7 py-5">
                <div className="min-w-0 flex-1">
                  <div className="eyebrow">Čuvam u</div>
                  <div className="mono selectable mt-1 text-[12.5px] break-all">{pv?.dest_tilde ?? `${proj.cwd_tilde}/claude-istorija`}/</div>
                </div>
                <button className="btn btn-primary btn-lg" disabled={!proj.exists || !pv} onClick={start}>
                  <FolderSearch size={17} /> Sačuvaj istoriju
                </button>
              </div>
            </div>
          )}
          {error && (
            <Callout tone="err" title="Nije uspelo">
              {error}
            </Callout>
          )}
          <Callout tone="info" icon={<GitBranch size={17} />}>
            U <span className="mono">claude-istorija/</span> pravim <span className="mono">.gitignore</span> sa „*” — istorija ne odlazi slučajno na GitHub. Svaki put se pravi novi, datirani backup; stari ostaju.
          </Callout>
        </div>
      </div>
    </div>
  );
}
