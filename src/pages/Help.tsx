import { ChevronDown } from "lucide-react";
import { useState, type ReactNode } from "react";
import { PageHead, rise } from "../components/ui";
import { isWin, T } from "../lib/platform";

const Q: [string, ReactNode][] = [
  [
    "Da li aplikacija nešto briše ili menja u Claude-u?",
    <>
      Ne. Claude fajlove samo čita. Jedine dve stvari koje uopšte piše van backupa: <b>čuvanje istorije</b> (samo ključ <span className="mono">cleanupPeriodDays</span> u <span className="mono">~/.claude/settings.json</span>, i to samo kad ti klikneš, uz kopiju starog fajla) i <b>vraćanje</b>, koje samo dodaje fajlove koji fale — nikad ne prepisuje postojeće. Automatski backup briše samo svoje stare automatske backupe, prema broju koji izabereš.
    </>,
  ],
  [
    "Kako znam da je backup tačan?",
    <>
      Svaki fajl se pri kopiranju čita jednom, zapisuje se njegov SHA-256 zbir, a na kraju se ceo backup ponovo pročita i svaki zbir uporedi. Ako se i jedan bajt razlikuje, vidiš crveni pečat. Proveru možeš da ponoviš kad god hoćeš u „Backupi i vraćanje” → Proveri.
    </>,
  ],
  [
    "Šta je sve u backupu?",
    <>
      Claude Code razgovori sa pod-agentima i memorijom projekata, slike i scratchpad, globalni CLAUDE.md, podešavanja, <span className="mono">~/.claude.json</span> (projekti i MCP serveri), istorija kucanih poruka, checkpoint snimci, skillovi/agenti/komande/pluginovi, Code tab kartice, Cowork sesije i podešavanja Claude aplikacije. Iz projekata samo CLAUDE.md, CLAUDE.local.md, .mcp.json i .claude/ — nikad kod.
    </>,
  ],
  [
    "Zašto u backupu nema login tokena?",
    <>
      <span className="mono">{isWin ? "%USERPROFILE%\\.claude\\.credentials.json" : "~/.claude/.credentials.json"}</span> se namerno preskače — to je tvoja prijava i ne treba da stoji u zip-u koji možda šalješ ili čuvaš na disku. Posle vraćanja na {T.newComputer} samo se ponovo prijaviš.
    </>,
  ],
  [
    "Kako da čitam razgovore iz backupa?",
    <>
      Otvori <span className="mono">Pregled.html</span> iz backupa (za .zip prvo dvoklik da se raspakuje). Tu su svi razgovori po projektima, sa pretragom. Za Obsidian, VS Code ili Notion koristi folder <span className="mono">citljivo-md/</span>.
    </>,
  ],
  [
    `Kako da prebacim sve na ${T.newComputer}?`,
    <>
      Na starom {T.computer === "Mac" ? "Mac-u" : "računaru"} napravi kompletan backup na eksterni disk ili {isWin ? "OneDrive" : "iCloud"}. Na novom instaliraj Claude i ovu aplikaciju, otvori „Backupi i vraćanje” → Otvori .zip… → Vrati izabrano. Projekte (kod) prebaci kao i inače; ako su na istoj putanji, razgovori se odmah povezuju.
    </>,
  ],
  [
    "Šta je folder claude-istorija u projektu?",
    <>
      Tu „Istorija projekta” čuva razgovore samo tog projekta. Unutra je <span className="mono">.gitignore</span> sa „*”, jer razgovori mogu da sadrže lozinke i API ključeve — tako ne odu slučajno na GitHub.
    </>,
  ],
  [
    "Automatski backup — moram li da držim aplikaciju otvorenu?",
    <>
      {isWin ? (
        <>
          Ne. Windows (Task Scheduler) je pokreće u zadato vreme u pozadini, bez prozora. Ako je računar spavao ili bio ugašen, backup se pravi čim ga upališ i prijaviš se. Po završetku stiže obaveštenje. Dnevnik je u <span className="mono">%APPDATA%\\ClaudeBackupTool\\claude-backup.log</span>.
        </>
      ) : (
        <>
          Ne. macOS (launchd) je pokreće u zadato vreme u pozadini, bez prozora. Ako je Mac spavao, backup se pravi kad se probudi; ako je bio ugašen, čim se uključiš i prijaviš. Po završetku stiže obaveštenje. Dnevnik je u <span className="mono">~/Library/Logs/claude-backup.log</span>.
        </>
      )}
    </>,
  ],
  [
    "Mogu li da koristim komandnu liniju?",
    <>
      Može — ista aplikacija radi i kao CLI:
      <pre className="mono selectable mt-2 overflow-x-auto rounded-lg bg-bg-2 p-3 text-[11.5px] leading-relaxed">
        {isWin
          ? `# PowerShell
$A = "$env:LOCALAPPDATA\\Claude Backup\\claude-backup.exe"
& $A --list | Out-Host
& $A --backup --dest "$HOME\\ClaudeBackups" | Out-Host
& $A --project-backup "$HOME\\Projects\\moj-projekat" | Out-Host
& $A --verify "$HOME\\Desktop\\ClaudeBackup_….zip" | Out-Host
& $A --restore "$HOME\\Desktop\\ClaudeBackup_….zip" | Out-Host`
          : `A="/Applications/Claude Backup.app/Contents/MacOS/claude-backup"
"$A" --list
"$A" --backup --dest ~/ClaudeBackups
"$A" --project-backup ~/Projects/moj-projekat
"$A" --verify ~/Desktop/ClaudeBackup_….zip
"$A" --restore ~/Desktop/ClaudeBackup_….zip`}
      </pre>
    </>,
  ],
  [
    "Da li radi sa backupima iz Python skripte?",
    <>
      Da. Format je isti (MANIFEST.json, SHA256SUMS.txt, original/…), pa aplikacija proverava i vraća backupe iz skripte, i obrnuto{isWin ? " — i između Mac-a i Windowsa." : ". Ako uključiš automatski backup ovde, on zamenjuje onaj iz skripte."}
    </>,
  ],
];

export function Help() {
  const [open, setOpen] = useState<number | null>(0);
  return (
    <div className="pb-10">
      <PageHead
        eyebrow="Pomoć"
        title={
          <>
            Česta <em className="display-wonk text-accent">pitanja</em>
          </>
        }
        lead="Kratko i jasno — kako aplikacija radi i šta (ne) dira."
      />
      <div className="card rise divide-y divide-line" style={rise(1)}>
        {Q.map(([q, a], i) => (
          <div key={i}>
            <button className="flex w-full items-center gap-4 px-6 py-4 text-left" onClick={() => setOpen(open === i ? null : i)}>
              <span className="mono w-6 text-[11px] text-accent">{String(i + 1).padStart(2, "0")}</span>
              <span className="flex-1 text-[14.5px] font-[580]">{q}</span>
              <ChevronDown size={17} className={`text-faint transition-transform ${open === i ? "rotate-180" : ""}`} />
            </button>
            {open === i && <div className="rise px-6 pb-5 pl-[64px] text-[13.5px] leading-relaxed text-muted">{a}</div>}
          </div>
        ))}
      </div>
    </div>
  );
}
