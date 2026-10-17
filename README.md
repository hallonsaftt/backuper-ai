# Claude Backup

Aplikacija za **Mac (Apple Silicon)** i **Windows 10/11** koja pravi potpun, proveren backup Claude Code i Cowork istorije: razgovore, memoriju projekata, podešavanja, skillove, MCP servere, slike iz razgovora.

- **Samo čita.** Claude fajlove nikad ne briše i ne menja.
- **Proverava sve.** Svaki fajl dobija SHA-256 zbir, a na kraju se ceo backup ponovo pročita i uporedi.
- **Originalni format + čitljivo.** Tačne kopije koje Claude ume da učita nazad, plus `Pregled.html` i Markdown.
- **Vraćanje samo dodaje.** Fajlovi koje već imaš se ne prepisuju.
- **Automatski backup.** Dnevno, nedeljno ili mesečno (macOS launchd / Windows Task Scheduler). Ako je računar bio ugašen, backup se napravi čim ga upališ.
- **Istorija jednog projekta.** Čuva se u `<projekat>/claude-istorija/`, uz `.gitignore` da ne ode na GitHub.
- **Čuvanje istorije.** Podešava `cleanupPeriodDays` da Claude Code ne briše stare razgovore posle 30 dana.

## Instalacija — Mac

Najlakše: otvori **Terminal** i nalepi jedan red. Skine poslednju verziju, ubaci je u Applications i otvori — bez „Open Anyway” i bez dodatnih komandi. Isti red služi i za ažuriranje.

Dok je repo privatan (treba [GitHub CLI](https://cli.github.com) i jednom `gh auth login`):
```
gh api repos/hallonsaftt/backuper-ai/contents/install.sh -H "Accept: application/vnd.github.raw" | bash
```

Ako je repo javan:
```
curl -fsSL https://raw.githubusercontent.com/hallonsaftt/backuper-ai/main/install.sh | bash
```

Ili skini .dmg u browseru i pusti skriptu da ga instalira: `bash install.sh ~/Downloads/Claude-Backup-AppleSilicon.dmg`

Ručno, preko .dmg-a:

1. Skini [Claude-Backup-AppleSilicon.dmg](https://github.com/hallonsaftt/backuper-ai/releases/latest/download/Claude-Backup-AppleSilicon.dmg) i prevuci **Claude Backup** u **Applications**.
2. Aplikacija nije potpisana Apple sertifikatom, pa je macOS prvi put blokira. U Terminalu:
   ```
   xattr -dr com.apple.quarantine "/Applications/Claude Backup.app"
   ```
   ili pokušaj da je otvoriš, pa **System Settings → Privacy & Security → Open Anyway**.

Za automatski backup preporuka: **System Settings → Privacy & Security → Full Disk Access** → dodaj Claude Backup (inače pozadinski backup ne može da čita projekte na Desktopu i u Documents). Posle ažuriranja ovo ponekad treba ponoviti.

## Instalacija — Windows

1. Skini [Claude-Backup-Windows-Setup.exe](https://github.com/hallonsaftt/backuper-ai/releases/latest/download/Claude-Backup-Windows-Setup.exe) i pokreni ga. Instalira se samo za tvog korisnika, bez admin prava.
2. Instaler nije potpisan sertifikatom, pa Windows SmartScreen može da kaže „Windows protected your PC” → klikni **More info → Run anyway**.

## Komandna linija

Ista aplikacija radi i bez prozora.

Mac:
```
A="/Applications/Claude Backup.app/Contents/MacOS/claude-backup"
"$A" --list
"$A" --backup --dest ~/ClaudeBackups
"$A" --project-backup ~/Projects/moj-projekat
"$A" --verify ~/Desktop/ClaudeBackup_….zip
"$A" --restore ~/Desktop/ClaudeBackup_….zip
"$A" --schedule daily --at 14:00 --keep 14
```

Windows (PowerShell):
```
$A = "$env:LOCALAPPDATA\Claude Backup\claude-backup.exe"
& $A --list | Out-Host
& $A --backup --dest "$HOME\ClaudeBackups" | Out-Host
& $A --schedule daily --at 14:00 --keep 14 | Out-Host
```

Format backupa je isti na oba sistema i isti kao kod Python skripte `claude_backup.py`, pa se backupi mogu proveravati i vraćati bilo gde.

## Razvoj

```
npm ci
npm run tauri dev            # aplikacija
npm run dev                  # samo interfejs u pregledaču, sa lažnim podacima
cargo test --manifest-path core/Cargo.toml
tests/e2e.sh <putanja do claude-backup> python/claude_backup.py   # E2E na lažnom home folderu
```

- `core/` — Rust jezgro (skeniranje, backup, provera, vraćanje, zakazivanje, CLI)
- `src-tauri/` — Tauri 2 sloj
- `src/` — React + Tailwind interfejs
- `python/claude_backup.py` — ista funkcionalnost kao terminal program (TUI), isti format backupa
- `tests/` — E2E test: backup → provera → vraćanje, uz proveru da izvorni fajlovi nisu dirani

Build za .dmg i .exe radi GitHub Actions (`.github/workflows/build.yml`) na svaki push na `main`, uz testove na pravom Mac-u i Windowsu.
