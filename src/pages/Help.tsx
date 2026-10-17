import { ChevronDown } from "lucide-react";
import { useState, type ReactNode } from "react";
import { PageHead, rise } from "../components/ui";
import { isSr, L } from "../lib/i18n";
import { isWin, T } from "../lib/platform";

const CREDS = isWin ? "%USERPROFILE%\\.claude\\.credentials.json" : "~/.claude/.credentials.json";
const SETTINGS = isWin ? "%USERPROFILE%\\.claude\\settings.json" : "~/.claude/settings.json";

function cli(project: string): string {
  return isWin
    ? `# PowerShell
$A = "$env:LOCALAPPDATA\\Claude Backup\\claude-backup.exe"
& $A --list | Out-Host
& $A --backup --dest "$HOME\\ClaudeBackups" | Out-Host
& $A --project-backup "$HOME\\Projects\\${project}" | Out-Host
& $A --verify "$HOME\\Desktop\\ClaudeBackup_….zip" | Out-Host
& $A --restore "$HOME\\Desktop\\ClaudeBackup_….zip" | Out-Host
& $A --list --lang sr | Out-Host     # en | sr`
    : `A="/Applications/Claude Backup.app/Contents/MacOS/claude-backup"
"$A" --list
"$A" --backup --dest ~/ClaudeBackups
"$A" --project-backup ~/Projects/${project}
"$A" --verify ~/Desktop/ClaudeBackup_….zip
"$A" --restore ~/Desktop/ClaudeBackup_….zip
"$A" --list --lang sr     # en | sr`;
}

const pre = (text: string) => <pre className="mono selectable mt-2 overflow-x-auto rounded-lg bg-bg-2 p-3 text-[11.5px] leading-relaxed">{text}</pre>;

function questionsSr(): [string, ReactNode][] {
  return [
    [
      "Da li aplikacija nešto briše ili menja u Claude-u?",
      <>
        Ne. Claude fajlove samo čita. Jedine dve stvari koje uopšte piše van backupa: <b>čuvanje istorije</b> (samo ključ <span className="mono">cleanupPeriodDays</span> u{" "}
        <span className="mono">{SETTINGS}</span>, i to samo kad ti klikneš, uz kopiju starog fajla) i <b>vraćanje</b>, koje samo dodaje fajlove koji fale — nikad ne prepisuje
        postojeće. Automatski backup briše samo svoje stare automatske backupe, prema broju koji izabereš.
      </>,
    ],
    [
      "Kako znam da je backup tačan?",
      <>
        Svaki fajl se pri kopiranju čita jednom, zapisuje se njegov SHA-256 zbir, a na kraju se ceo backup ponovo pročita i svaki zbir uporedi. Ako se i jedan bajt razlikuje, vidiš crveni pečat.
        Proveru možeš da ponoviš kad god hoćeš u „Backupi i vraćanje” → Proveri.
      </>,
    ],
    [
      "Šta je sve u backupu?",
      <>
        Claude Code razgovori sa pod-agentima i memorijom projekata, slike i scratchpad, globalni CLAUDE.md, podešavanja, <span className="mono">~/.claude.json</span> (projekti i MCP serveri),
        istorija kucanih poruka, checkpoint snimci, skillovi/agenti/komande/pluginovi, Code tab kartice, Cowork sesije i podešavanja Claude aplikacije. Iz projekata samo CLAUDE.md,
        CLAUDE.local.md, .mcp.json i .claude/ — nikad kod.
      </>,
    ],
    [
      "Zašto u backupu nema login tokena?",
      <>
        <span className="mono">{CREDS}</span> se namerno preskače — to je tvoja prijava i ne treba da stoji u zip-u koji možda šalješ ili čuvaš na disku. Posle vraćanja na {T.newComputer} samo se
        ponovo prijaviš.
      </>,
    ],
    [
      "Kako da čitam razgovore iz backupa?",
      <>
        Otvori <span className="mono">Pregled.html</span> iz backupa (za .zip prvo dvoklik da se raspakuje). Tu su svi razgovori po projektima, sa pretragom. Za Obsidian, VS Code ili Notion
        koristi folder <span className="mono">citljivo-md/</span>. Backupi napravljeni na engleskom imaju iste fajlove pod imenima <span className="mono">Overview.html</span> i{" "}
        <span className="mono">readable-md/</span>.
      </>,
    ],
    [
      `Kako da prebacim sve na ${T.newComputer}?`,
      <>
        Na starom {isWin ? "računaru" : "Mac-u"} napravi kompletan backup na eksterni disk ili {isWin ? "OneDrive" : "iCloud"}. Na novom instaliraj Claude i ovu aplikaciju, otvori „Backupi i vraćanje”
        → Otvori .zip… → Vrati izabrano. Projekte (kod) prebaci kao i inače; ako su na istoj putanji, razgovori se odmah povezuju.
      </>,
    ],
    [
      "Šta je folder claude-istorija u projektu?",
      <>
        Tu „Istorija projekta” čuva razgovore samo tog projekta. Unutra je <span className="mono">.gitignore</span> sa „*”, jer razgovori mogu da sadrže lozinke i API ključeve — tako ne odu
        slučajno na GitHub. Na engleskom se folder zove <span className="mono">claude-history</span>; ako projekat već ima jedan od ta dva foldera, koristi se postojeći.
      </>,
    ],
    [
      "Automatski backup — moram li da držim aplikaciju otvorenu?",
      isWin ? (
        <>
          Ne. Windows (Task Scheduler) je pokreće u zadato vreme u pozadini, bez prozora. Ako je računar spavao ili bio ugašen, backup se pravi čim ga upališ i prijaviš se. Po završetku stiže
          obaveštenje. Dnevnik je u <span className="mono">%APPDATA%\ClaudeBackupTool\claude-backup.log</span>.
        </>
      ) : (
        <>
          Ne. macOS (launchd) je pokreće u zadato vreme u pozadini, bez prozora. Ako je Mac spavao, backup se pravi kad se probudi; ako je bio ugašen, čim se uključiš i prijaviš. Po završetku
          stiže obaveštenje. Dnevnik je u <span className="mono">~/Library/Logs/claude-backup.log</span>.
        </>
      ),
    ],
    [
      "Kako da promenim jezik?",
      <>
        Dole levo, pored verzije: <b>EN | SR</b>. Izbor se pamti i važi za aplikaciju, obaveštenja automatskog backupa i komandnu liniju (tamo može i <span className="mono">--lang en</span> ili{" "}
        <span className="mono">--lang sr</span>). Novi backupi dobijaju uputstvo i pregled na izabranom jeziku; stari ostaju kakvi jesu i proveravaju se i vraćaju isto.
      </>,
    ],
    ["Mogu li da koristim komandnu liniju?", <>Može — ista aplikacija radi i kao CLI:{pre(cli("moj-projekat"))}</>],
    [
      "Da li radi sa backupima iz Python skripte?",
      <>
        Da. Format je isti (MANIFEST.json, SHA256SUMS.txt, original/…), pa aplikacija proverava i vraća backupe iz skripte, i obrnuto
        {isWin ? " — i između Mac-a i Windowsa." : ". Ako uključiš automatski backup ovde, on zamenjuje onaj iz skripte."}
      </>,
    ],
  ];
}

function questionsEn(): [string, ReactNode][] {
  return [
    [
      "Does the app delete or change anything in Claude?",
      <>
        No. It only reads Claude's files. The only two things it ever writes outside a backup: <b>history retention</b> (just the <span className="mono">cleanupPeriodDays</span> key in{" "}
        <span className="mono">{SETTINGS}</span>, only when you click, with a copy of the old file) and <b>restore</b>, which only adds missing files — it never overwrites existing ones.
        Automatic backup deletes only its own old automatic backups, down to the number you choose.
      </>,
    ],
    [
      "How do I know a backup is accurate?",
      <>
        Every file is read once while copying and its SHA-256 sum is recorded; at the end the whole backup is read again and every sum compared. If even one byte differs, you see a red seal.
        You can repeat the check any time in “Backups & restore” → Check.
      </>,
    ],
    [
      "What's in a backup?",
      <>
        Claude Code conversations with subagents and project memory, images and scratchpad, the global CLAUDE.md, settings, <span className="mono">~/.claude.json</span> (projects and MCP servers),
        your typed-prompt history, checkpoints, skills/agents/commands/plugins, Code tab cards, Cowork sessions and the Claude app's settings. From projects only CLAUDE.md, CLAUDE.local.md, .mcp.json
        and .claude/ — never code.
      </>,
    ],
    [
      "Why isn't the login token in the backup?",
      <>
        <span className="mono">{CREDS}</span> is skipped on purpose — it's your sign-in and doesn't belong in a zip you might send or keep on a disk. After restoring on {T.newComputer} you just
        sign in again.
      </>,
    ],
    [
      "How do I read conversations from a backup?",
      <>
        Open <span className="mono">Overview.html</span> from the backup (for a .zip, double-click it first to unpack). Every conversation is there, by project, with search. For Obsidian, VS Code or
        Notion use the <span className="mono">readable-md/</span> folder. Backups made in Serbian have the same files named <span className="mono">Pregled.html</span> and{" "}
        <span className="mono">citljivo-md/</span>.
      </>,
    ],
    [
      `How do I move everything to ${T.newComputer}?`,
      <>
        On the old {T.computer}, make a full backup to an external drive or {isWin ? "OneDrive" : "iCloud"}. On the new one, install Claude and this app, open “Backups & restore” → Open .zip… →
        Restore selected. Move your projects (code) as usual; if they're at the same path, the conversations connect right away.
      </>,
    ],
    [
      "What's the claude-history folder in a project?",
      <>
        That's where “Project history” keeps the conversations of just that project. It has a <span className="mono">.gitignore</span> with “*”, because conversations can contain passwords and
        API keys — so they can't end up on GitHub by accident. In Serbian the folder is called <span className="mono">claude-istorija</span>; if a project already has either one, the existing
        folder is used.
      </>,
    ],
    [
      "Automatic backup — do I have to keep the app open?",
      isWin ? (
        <>
          No. Windows (Task Scheduler) starts it at the set time in the background, with no window. If the computer was asleep or off, the backup runs as soon as you turn it on and sign in. A
          notification arrives when it's done. The log is in <span className="mono">%APPDATA%\ClaudeBackupTool\claude-backup.log</span>.
        </>
      ) : (
        <>
          No. macOS (launchd) starts it at the set time in the background, with no window. If the Mac was asleep, the backup runs when it wakes; if it was off, as soon as you turn it on and sign
          in. A notification arrives when it's done. The log is in <span className="mono">~/Library/Logs/claude-backup.log</span>.
        </>
      ),
    ],
    [
      "How do I change the language?",
      <>
        Bottom left, next to the version: <b>EN | SR</b>. The choice is remembered and applies to the app, automatic-backup notifications and the command line (which also takes{" "}
        <span className="mono">--lang en</span> or <span className="mono">--lang sr</span>). New backups get their guide and overview in the chosen language; old ones stay as they are and are
        checked and restored the same way.
      </>,
    ],
    ["Can I use the command line?", <>Yes — the same app also works as a CLI:{pre(cli("my-project"))}</>],
    [
      "Does it work with backups from the Python script?",
      <>
        Yes. The format is the same (MANIFEST.json, SHA256SUMS.txt, original/…), so the app checks and restores the script's backups, and the other way round
        {isWin ? " — even between a Mac and Windows." : ". If you turn on automatic backup here, it replaces the script's one."}
      </>,
    ],
  ];
}

export function Help() {
  const [open, setOpen] = useState<number | null>(0);
  const Q = isSr() ? questionsSr() : questionsEn();
  return (
    <div className="pb-10">
      <PageHead
        eyebrow={L("Pomoć", "Help")}
        title={
          <>
            {L("Česta", "Common")} <em className="display-wonk text-accent">{L("pitanja", "questions")}</em>
          </>
        }
        lead={L("Kratko i jasno — kako aplikacija radi i šta (ne) dira.", "Short and clear — how the app works and what it does (and doesn't) touch.")}
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
