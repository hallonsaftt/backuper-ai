import { FileArchive, Folder, FolderInput, FolderSearch, GitBranch, Search, TerminalSquare } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { call, pickFolder } from "../api";
import { BackupResultView } from "../components/ResultView";
import { Callout, cx, KV, PageHead, rise, Seg, Spinner, Switch } from "../components/ui";
import { useJob } from "../job";
import { ago, agoShort, human, nConvs, num } from "../lib/format";
import { L } from "../lib/i18n";
import { SEP } from "../lib/platform";
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
  // claude-history (en) or claude-istorija (sr); an existing folder of either name is reused — the backend decides
  const dirName = pv?.dest_tilde.split(/[\\/]/).pop() || L("claude-istorija", "claude-history");

  const fromFolder = async () => {
    setNotFound(null);
    const path = await pickFolder(L("Izaberi folder projekta", "Choose the project folder"));
    if (!path) return;
    const key = await call<string | null>("project_for_folder", { path });
    if (key) setSel(key);
    else setNotFound(path);
  };

  const start = async () => {
    if (!sel) return;
    setError(null);
    const d = await job.run<BackupDone>("start_project_backup", { key: sel, html, md, zip }, { name: "backup", title: `${L("Čuvam istoriju", "Saving history")} — ${proj?.name ?? ""}`, cancellable: true });
    if (d.ok) {
      setDone(d.data);
      onDone();
    } else setError(d.error);
  };

  if (done)
    return (
      <BackupResultView
        done={done}
        title={L(`Istorija projekta ${proj?.name ?? ""}`, `History of ${proj?.name ?? ""}`)}
        onBack={() => setDone(null)}
        extra={
          <div className="rise mt-5">
            <Callout tone="info" icon={<GitBranch size={17} />} title={L(`Folder ${dirName} je van git-a`, `The ${dirName} folder is kept out of git`)}>
              {L(
                "U njemu je .gitignore sa „*”, jer razgovori mogu da sadrže lozinke i API ključeve. Ako ipak želiš da ga deliš, prvo pregledaj sadržaj.",
                "It has a .gitignore with “*”, because conversations can contain passwords and API keys. If you still want to share it, review the contents first.",
              )}
            </Callout>
          </div>
        }
      />
    );

  return (
    <div>
      <PageHead
        eyebrow={L("Istorija projekta", "Project history")}
        title={
          <>
            {L("Sačuvaj istoriju", "Save the history")} <em className="display-wonk text-accent">{L("u sam projekat", "into the project itself")}</em>
          </>
        }
        lead={L(
          `Izaberi projekat — sve razgovore, memoriju, slike i Claude fajlove tog projekta sačuvaću u njegov folder ${dirName}/. Ostali projekti se ne diraju.`,
          `Pick a project — all of its conversations, memory, images and Claude files are saved into its ${dirName}/ folder. Other projects aren't touched.`,
        )}
      />

      <div className="rise grid grid-cols-[320px_minmax(0,1fr)] gap-5" style={rise(1)}>
        {/* list */}
        <div className="card flex max-h-[calc(100vh-250px)] min-h-[420px] flex-col overflow-hidden">
          <div className="space-y-2.5 border-b border-line p-3.5">
            <button className="btn btn-ink w-full" onClick={fromFolder}>
              <FolderInput size={16} /> {L("Izaberi folder projekta…", "Choose a project folder…")}
            </button>
            <div className="relative">
              <Search size={14} className="absolute top-1/2 left-3 -translate-y-1/2 text-faint" />
              <input className="field h-[34px] w-full pl-8 text-[13px]" placeholder={L(`Traži među ${eligible.length} projekata…`, `Search ${eligible.length} projects…`)} value={q} onChange={(e) => setQ(e.target.value)} />
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
                    <div className={on ? "text-bg/50" : "text-faint"}>{agoShort(p.last)}</div>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* details */}
        <div className="space-y-4">
          {notFound && (
            <Callout tone="warn" title={L("Taj folder nije Claude Code projekat", "That folder isn't a Claude Code project")}>
              <span className="mono">{notFound}</span>{" "}
              {L("— za njega nema sačuvanih razgovora. Izaberi folder u kome si pokretao Claude.", "— there are no saved conversations for it. Choose a folder where you ran Claude.")}
            </Callout>
          )}
          {proj && (
            <div className="card overflow-hidden">
              <div className="px-7 pt-6 pb-5">
                <div className="eyebrow">{L("Projekat", "Project")}</div>
                <h2 className="display mt-1.5 text-[30px] leading-tight font-[460]">{proj.name}</h2>
                <div className="mono selectable mt-1 text-[12px] text-muted">{proj.cwd_tilde}</div>
                <div className="num mt-3 text-[14px]">
                  <b className="font-[650]">{nConvs(proj.sessions)}</b>
                  {pv && <span className="text-muted"> · {human(pv.sessions_size)} · {L("poslednji", "last")} {ago(proj.last)}</span>}
                </div>
                {!proj.exists && (
                  <div className="mt-3">
                    <Callout tone="warn" title={L("Folder projekta više ne postoji", "The project folder no longer exists")}>
                      {L(
                        "Razgovori su i dalje u Claude-u, ali nemam gde da ih sačuvam u projekat. Napravi kompletan backup umesto toga.",
                        "The conversations are still in Claude, but there's nowhere to save them inside the project. Make a full backup instead.",
                      )}
                    </Callout>
                  </div>
                )}
              </div>
              <div className="grid grid-cols-2 gap-x-8 border-t border-line px-7 py-4">
                {pv ? (
                  <>
                    <KV k={L("Kucane poruke", "Typed prompts")} v={num(pv.prompts)} />
                    <KV k={L("Slike, scratchpad", "Images, scratchpad")} v={num(pv.images)} />
                    <KV k={L("Rewind, zadaci", "Rewind, tasks")} v={num(pv.per_session)} />
                    <KV k={L("Code tab kartice", "Code tab cards")} v={num(pv.cards)} />
                    <KV k={L("Memorija", "Memory")} v={num(pv.memory)} />
                    <KV k={L("Claude fajlovi", "Claude files")} v={num(pv.project_files)} />
                  </>
                ) : (
                  <div className="col-span-2 flex items-center gap-2 py-3 text-[13px] text-muted">
                    <Spinner size={14} /> {L("Brojim…", "Counting…")}
                  </div>
                )}
              </div>
              <div className="space-y-3 border-t border-line bg-card-2 px-7 py-4">
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">{L("Pregled.html", "Overview.html")}</span>
                  <Switch on={html} onChange={setHtml} />
                </div>
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">Markdown (.md)</span>
                  <Switch on={md} onChange={setMd} />
                </div>
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px]">{L("Pakovanje", "Packaging")}</span>
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
                  <div className="eyebrow">{L("Čuvam u", "Saving to")}</div>
                  <div className="mono selectable mt-1 text-[12.5px] break-all">
                    {pv?.dest_tilde ?? `${proj.cwd_tilde}${SEP}${dirName}`}
                    {SEP}
                  </div>
                </div>
                <button className="btn btn-primary btn-lg" disabled={!proj.exists || !pv} onClick={start}>
                  <FolderSearch size={17} /> {L("Sačuvaj istoriju", "Save history")}
                </button>
              </div>
            </div>
          )}
          {error && (
            <Callout tone="err" title={L("Nije uspelo", "That didn't work")}>
              {error}
            </Callout>
          )}
          <Callout tone="info" icon={<GitBranch size={17} />}>
            {L("U", "Inside")} <span className="mono">{dirName}/</span> {L("pravim", "I put a")} <span className="mono">.gitignore</span>{" "}
            {L(
              "sa „*” — istorija ne odlazi slučajno na GitHub. Svaki put se pravi novi, datirani backup; stari ostaju.",
              "with “*” — the history can't end up on GitHub by accident. Each run makes a new, dated backup; old ones stay.",
            )}
          </Callout>
        </div>
      </div>
    </div>
  );
}
