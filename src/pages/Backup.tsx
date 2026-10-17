import { FileArchive, Folder, PackagePlus, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { call } from "../api";
import { DestPicker } from "../components/DestPicker";
import { BackupResultView } from "../components/ResultView";
import { Callout, Check, cx, PageHead, rise, Section, Seg, Switch } from "../components/ui";
import { useJob } from "../job";
import { ago, human, nConvs, nFiles } from "../lib/format";
import type { BackupDone, Estimate, Overview } from "../types";
import { T } from "../lib/platform";
import { L } from "../lib/i18n";

const PRESETS: [() => string, string, (id: string) => boolean][] = [
  [() => L("Preporučeno", "Recommended"), "rec", () => true],
  [() => L("Samo Claude Code", "Claude Code only"), "cc", (id) => ["code_sessions", "tmp_sessions", "global", "extensions", "project_files", "desktop_code"].includes(id)],
  [() => L("Samo Cowork", "Cowork only"), "cw", (id) => ["cowork", "desktop_config"].includes(id)],
  [() => L("Sve", "Everything"), "all", () => true],
];

export function Backup({ ov, onDone }: { ov: Overview; onDone: () => void }) {
  const job = useJob();
  const [cats, setCats] = useState<string[]>(() => ov.cats.filter((c) => c.default).map((c) => c.id));
  const [preset, setPreset] = useState("rec");
  const [allProjects, setAllProjects] = useState(true);
  const [projects, setProjects] = useState<string[]>(() => ov.projects.map((p) => p.key));
  const [q, setQ] = useState("");
  const [html, setHtml] = useState(true);
  const [md, setMd] = useState(true);
  const [zip, setZip] = useState(true);
  const [dest, setDest] = useState(() => ov.dests.find((d) => d.label === "Desktop")?.path ?? ov.dests[0]?.path ?? "");
  const [est, setEst] = useState<Estimate | null>(null);
  const [done, setDone] = useState<BackupDone | null>(null);
  const [error, setError] = useState<string | null>(null);

  const projArg = allProjects ? null : projects;
  useEffect(() => {
    let live = true;
    call<Estimate>("estimate", { cats, projects: projArg }).then((e) => live && setEst(e));
    return () => {
      live = false;
    };
  }, [cats.join(","), allProjects, projects.join(",")]); // eslint-disable-line

  const applyPreset = (id: string) => {
    setPreset(id);
    const f = PRESETS.find((p) => p[1] === id)![2];
    setCats(ov.cats.filter((c) => c.found && (id === "rec" ? c.default : f(c.id))).map((c) => c.id));
  };
  const toggle = (id: string) => {
    setPreset("");
    setCats((cs) => (cs.includes(id) ? cs.filter((x) => x !== id) : [...cs, id]));
  };

  const shown = useMemo(() => ov.projects.filter((p) => !q || (p.name + p.cwd_tilde).toLowerCase().includes(q.toLowerCase())), [ov.projects, q]);

  const start = async () => {
    setError(null);
    const d = await job.run<BackupDone>("start_backup", { req: { cats, projects: projArg, html, md, dest, zip } }, { name: "backup", title: L("Pravim backup", "Making the backup"), cancellable: true });
    if (d.ok) {
      setDone(d.data);
      onDone();
    } else setError(d.error);
  };

  if (done) return <BackupResultView done={done} title={L("Tvoja Claude istorija", "Your Claude history")} onBack={() => setDone(null)} />;

  const canStart = cats.length > 0 && !!dest && (allProjects || projects.length > 0);

  return (
    <div className="pb-28">
      <PageHead
        eyebrow={L("Novi backup", "New backup")}
        title={
          <>
            {L("Šta da", "What should I")} <em className="display-wonk text-accent">{L("sačuvam", "save")}</em>?
          </>
        }
        lead={L(
          "Sve je već izabrano kako treba — samo izaberi gde da sačuvam i klikni dugme dole. Claude fajlove samo čitam; ništa se ne briše i ne menja.",
          "Everything is already selected the right way — just choose where to save and click the button below. Claude's files are only read; nothing is deleted or changed.",
        )}
      />

      <Section n="01" title={L("Sadržaj", "Contents")} i={1} right={
        <div className="flex gap-1.5">
          {PRESETS.map(([label, id]) => (
            <button key={id} className="chip" data-on={preset === id} onClick={() => applyPreset(id)}>
              {label()}
            </button>
          ))}
        </div>
      }>
        <div className="grid grid-cols-2 gap-2.5">
          {ov.cats.map((c, i) => {
            const on = cats.includes(c.id);
            return (
              <button
                key={c.id}
                disabled={!c.found}
                onClick={() => toggle(c.id)}
                className={cx(
                  "rise flex gap-3.5 rounded-xl border p-4 text-left transition-all",
                  on ? "border-ink/25 bg-card shadow-[var(--shadow-card)]" : "border-line bg-transparent hover:bg-card/60",
                  !c.found && "opacity-45",
                )}
                style={rise(1 + i * 0.4)}
              >
                <div className="pt-0.5">
                  <Check on={on} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="text-[14px] font-[600]">{c.title}</span>
                  </div>
                  <div className="mt-1 text-[12.5px] leading-snug text-muted">{c.desc}</div>
                  <div className={cx("num mt-2 text-[11.5px]", c.found ? "text-ink/70" : "text-faint")}>{c.note}</div>
                </div>
              </button>
            );
          })}
        </div>
      </Section>

      <Section n="02" title={L("Projekti", "Projects")} hint={L("za razgovore, slike i Claude fajlove iz projekata", "for conversations, images and Claude files from projects")} i={2}>
        <div className="card overflow-hidden">
          <div className="flex items-center gap-3 border-b border-line px-5 py-3.5">
            <Seg
              value={allProjects ? "all" : "some"}
              onChange={(v) => setAllProjects(v === "all")}
              options={[
                ["all", `${L("Svi projekti", "All projects")} (${ov.projects.length})`],
                ["some", L("Samo izabrani", "Selected only")],
              ]}
            />
            {!allProjects && (
              <>
                <div className="relative ml-auto">
                  <Search size={14} className="absolute top-1/2 left-3 -translate-y-1/2 text-faint" />
                  <input className="field h-[34px] w-[220px] pl-8 text-[13px]" placeholder={L("Traži projekat…", "Search projects…")} value={q} onChange={(e) => setQ(e.target.value)} />
                </div>
                <button className="btn btn-quiet" onClick={() => setProjects(projects.length ? [] : ov.projects.map((p) => p.key))}>
                  {projects.length ? L("Poništi", "Clear") : L("Sve", "All")}
                </button>
              </>
            )}
          </div>
          {!allProjects && (
            <div className="scroll max-h-[260px] divide-y divide-line">
              {shown.map((p) => {
                const on = projects.includes(p.key);
                return (
                  <button key={p.key} onClick={() => setProjects((ps) => (on ? ps.filter((x) => x !== p.key) : [...ps, p.key]))} className="flex w-full items-center gap-3.5 px-5 py-2.5 text-left hover:bg-card-2">
                    <Check on={on} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13.5px] font-[560]">{p.name}</div>
                      <div className="mono truncate text-[11px] text-faint">{p.cwd_tilde}</div>
                    </div>
                    <span className="num text-[12px] text-muted">{nConvs(p.sessions)}</span>
                    <span className="w-[90px] text-right text-[12px] text-faint">{ago(p.last)}</span>
                  </button>
                );
              })}
            </div>
          )}
          {allProjects && <div className="px-5 py-3 text-[12.5px] text-muted">{L("Sve što pripada bilo kom projektu ide u backup — preporučeno.", "Everything that belongs to any project goes into the backup — recommended.")}</div>}
        </div>
      </Section>

      <Section n="03" title="Format" i={3}>
        <div className="card divide-y divide-line">
          <Row
            title={L("Originalni Claude format", "Original Claude format")}
            desc={L(
              "Tačne kopije .jsonl i ostalih fajlova — to Claude ume da učita nazad. Uvek uključeno.",
              "Exact copies of the .jsonl and other files — what Claude can load back. Always on.",
            )}
          >
            <Check on locked />
          </Row>
          <Row
            title={L("Pregled.html", "Overview.html")}
            desc={L("Lepa stranica sa svim razgovorima — otvori u pregledaču i čitaj, bez Claude-a.", "A nice page with every conversation — open it in a browser and read, no Claude needed.")}
          >
            <Switch on={html} onChange={setHtml} />
          </Row>
          <Row
            title="Markdown (.md)"
            desc={L("Svaki razgovor kao .md fajl — za Obsidian, Notion, VS Code ili pretragu.", "Every conversation as an .md file — for Obsidian, Notion, VS Code or search.")}
          >
            <Switch on={md} onChange={setMd} />
          </Row>
          <Row
            title={L("Pakovanje", "Packaging")}
            desc={
              zip
                ? L("Jedan .zip fajl — lako se premešta i čuva.", "A single .zip file — easy to move and keep.")
                : L(`Običan folder — sve odmah vidljivo u ${T.inFm}.`, `A plain folder — everything visible right away in ${T.inFm}.`)
            }
          >
            <Seg
              value={zip ? "zip" : "dir"}
              onChange={(v) => setZip(v === "zip")}
              options={[
                ["zip", <span className="flex items-center gap-1.5"><FileArchive size={14} /> ZIP</span>],
                ["dir", <span className="flex items-center gap-1.5"><Folder size={14} /> Folder</span>],
              ]}
            />
          </Row>
        </div>
      </Section>

      <Section n="04" title={L("Gde da sačuvam", "Where to save")} i={4}>
        <DestPicker dests={ov.dests} value={dest} onChange={setDest} />
      </Section>

      {error && (
        <div className="mb-6">
          <Callout tone="err" title={L("Backup nije napravljen", "The backup wasn't made")}>
            {error}
          </Callout>
        </div>
      )}

      {/* bottom bar */}
      <div className="fixed right-0 bottom-0 left-[244px] z-20 border-t border-line-2 bg-bg/85 backdrop-blur-md">
        <div className="mx-auto flex max-w-[980px] items-center gap-5 px-10 py-3.5">
          <div className="min-w-0 flex-1">
            <div className="eyebrow">{L("Ukupno", "Total")}</div>
            <div className="num mt-0.5 text-[15px] font-[600]">
              {est ? (
                <>
                  {nFiles(est.files)} <span className="text-muted">·</span> {human(est.bytes)} <span className="text-muted">·</span> <span className="font-[500] text-muted">{nConvs(est.convs)}</span>
                </>
              ) : (
                "…"
              )}
            </div>
          </div>
          <button className="btn btn-primary btn-lg" disabled={!canStart} onClick={start}>
            <PackagePlus size={18} /> {L("Napravi backup", "Back up now")}
          </button>
        </div>
      </div>
    </div>
  );
}

function Row({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-6 px-5 py-4">
      <div className="min-w-0 flex-1">
        <div className="text-[14px] font-[600]">{title}</div>
        <div className="mt-0.5 text-[12.5px] text-muted">{desc}</div>
      </div>
      {children}
    </div>
  );
}
