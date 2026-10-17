import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { call } from "./api";
import { JobOverlay } from "./components/JobOverlay";
import { Sidebar } from "./components/Sidebar";
import { Seal } from "./components/ui";
import { JobProvider, useJob } from "./job";
import { getLang, L, setLangLocal, type Lang } from "./lib/i18n";
import { Archive } from "./pages/Archive";
import { Backup } from "./pages/Backup";
import { Help } from "./pages/Help";
import { Overview } from "./pages/Overview";
import { Project } from "./pages/Project";
import { Retention } from "./pages/Retention";
import { Schedule } from "./pages/Schedule";
import type { Overview as OV, Page } from "./types";

export function App() {
  return (
    <JobProvider>
      <Shell />
    </JobProvider>
  );
}

function Shell() {
  const job = useJob();
  const [ov, setOv] = useState<OV | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [page, setPage] = useState<Page>(() => (new URLSearchParams(location.search).get("page") as Page) || "overview");
  const [projKey, setProjKey] = useState<string | null>(null);
  const [version, setVersion] = useState("1.0.0");
  const [nonce, setNonce] = useState(0);
  const [lang, setLang] = useState<Lang>(getLang());
  const scroller = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    setErr(null);
    try {
      setOv(await call<OV>("overview"));
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    document.documentElement.lang = getLang();
    // the backend's saved choice wins (it also drives scheduled backups)
    call<string>("get_lang")
      .then((l) => {
        if ((l === "en" || l === "sr") && l !== getLang()) {
          setLangLocal(l);
          setLang(l);
        }
      })
      .catch(() => {})
      .finally(load);
    call<string>("app_version").then((v) => v && setVersion(v)).catch(() => {});
  }, [load]);

  const changeLang = async (l: Lang) => {
    if (l === lang) return;
    try {
      await call("set_lang", { lang: l });
    } catch {
      /* still switch the screen */
    }
    setLangLocal(l);
    setLang(l);
    setNonce((n) => n + 1);
    call<OV>("overview").then(setOv).catch(() => {});
    call<string>("app_version").then((v) => v && setVersion(v)).catch(() => {});
  };

  const go = (p: Page) => {
    setPage(p);
    setNonce((n) => n + 1);
    scroller.current?.scrollTo({ top: 0 });
  };
  const refreshQuiet = () => {
    call<OV>("overview").then(setOv).catch(() => {});
  };

  return (
    <div className="flex h-full">
      <Sidebar page={page} onNav={go} ov={ov} locked={job.active} version={version} lang={lang} onLang={changeLang} />
      <main className="grain relative min-w-0 flex-1 bg-bg">
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-20 h-9" />
        <div ref={scroller} className="scroll relative z-10 h-full">
          <div className="mx-auto max-w-[980px] px-10 pt-14 pb-12">
            {!ov && !err && <Loading />}
            {err && (
              <div className="pt-20 text-center">
                <div className="display text-[26px]">{L("Nisam uspeo da pregledam Claude fajlove", "Couldn't scan your Claude files")}</div>
                <p className="mono mt-2 text-[12px] text-muted">{err}</p>
                <button className="btn btn-ghost mt-5" onClick={load}>
                  <RefreshCw size={15} /> {L("Pokušaj ponovo", "Try again")}
                </button>
              </div>
            )}
            {ov && (
              <div key={`${page}-${nonce}-${lang}`}>
                {page === "overview" && (
                  <Overview
                    ov={ov}
                    go={go}
                    refresh={() => {
                      setOv(null);
                      load();
                    }}
                    openProject={(k) => {
                      setProjKey(k);
                      go("project");
                    }}
                  />
                )}
                {page === "backup" && <Backup ov={ov} onDone={refreshQuiet} />}
                {page === "project" && <Project ov={ov} initial={projKey} onDone={refreshQuiet} />}
                {page === "archive" && <Archive ov={ov} onChanged={refreshQuiet} />}
                {page === "schedule" && <Schedule ov={ov} onChanged={refreshQuiet} />}
                {page === "retention" && <Retention ov={ov} onChanged={refreshQuiet} />}
                {page === "help" && <Help />}
              </div>
            )}
          </div>
        </div>
        {job.active && <JobOverlay />}
      </main>
    </div>
  );
}

function Loading() {
  return (
    <div className="grid place-items-center pt-28 text-center">
      <div className="spin opacity-70">
        <Seal size={120} animate={false} label="" />
      </div>
      <div className="display mt-8 text-[24px] font-[440]">{L("Pregledam tvoju Claude arhivu…", "Looking through your Claude archive…")}</div>
      <p className="mt-1.5 text-[13px] text-muted">{L("Brojim razgovore, projekte i podešavanja. Ništa ne diram.", "Counting conversations, projects and settings. Nothing gets touched.")}</p>
    </div>
  );
}
