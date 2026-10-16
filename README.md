# Claude Backup

Mac aplikacija (Apple Silicon) koja pravi potpun, proveren backup Claude Code i Cowork istorije: razgovore, memoriju projekata, podešavanja, skillove, MCP servere, slike iz razgovora.

- **Samo čita.** Claude fajlove nikad ne briše i ne menja.
- **Proverava sve.** Svaki fajl dobija SHA-256 zbir, a na kraju se ceo backup ponovo pročita i uporedi.
- **Originalni format + čitljivo.** Tačne kopije koje Claude ume da učita nazad, plus `Pregled.html` i Markdown.
- **Vraćanje samo dodaje.** Fajlovi koje već imaš se ne prepisuju.
- **Automatski backup.** Dnevno, nedeljno ili mesečno preko macOS launchd-a. Ako je Mac bio ugašen, backup se napravi čim ga upališ.
- **Istorija jednog projekta.** Čuva se u `<projekat>/claude-istorija/`, uz `.gitignore` da ne ode na GitHub.
- **Čuvanje istorije.** Podešava `cleanupPeriodDays` da Claude Code ne briše stare razgovore posle 30 dana.

## Instalacija

1. Skini `Claude-Backup-AppleSilicon.dmg` sa [Releases](../../releases/latest).
2. Otvori .dmg i prevuci **Claude Backup** u **Applications**.
3. Aplikacija nije potpisana Apple sertifikatom, pa je macOS prvi put blokira. Otvori Terminal i pokreni:
   ```
   xattr -dr com.apple.quarantine "/Applications/Claude Backup.app"
   ```
   Ili: pokušaj da je otvoriš, pa idi u **System Settings → Privacy & Security** i klikni **Open Anyway**.
4. (Preporuka, za automatski backup) **System Settings → Privacy & Security → Full Disk Access** → dodaj Claude Backup. Bez toga pozadinski backup ne može da čita projekte na Desktopu i u Documents.

## Komandna linija

Ista aplikacija radi i bez prozora:

```
A="/Applications/Claude Backup.app/Contents/MacOS/claude-backup"
"$A" --list
"$A" --backup --dest ~/ClaudeBackups
"$A" --project-backup ~/Projects/moj-projekat
"$A" --verify ~/Desktop/ClaudeBackup_….zip
"$A" --restore ~/Desktop/ClaudeBackup_….zip
"$A" --schedule daily --at 14:00 --keep 14
```

Format backupa je isti kao kod Python skripte `claude_backup.py`, pa aplikacija čita i vraća i njene backupe (i obrnuto).

## Razvoj

```
npm ci
npm run tauri dev            # aplikacija
npm run dev                  # samo interfejs u pregledaču, sa lažnim podacima
cargo test --manifest-path core/Cargo.toml
```

- `core/` — Rust jezgro (skeniranje, backup, provera, vraćanje, zakazivanje, CLI)
- `src-tauri/` — Tauri 2 sloj
- `src/` — React + Tailwind interfejs

Build za .dmg radi GitHub Actions (`.github/workflows/build-macos.yml`) na svaki push na `main`.
