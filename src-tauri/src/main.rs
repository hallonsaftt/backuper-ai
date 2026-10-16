// Bez konzolnog prozora na Windows-u (ne koristi se, ali ne smeta).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Zakazani backup (launchd) i komandna linija: radi bez prozora pa izađe.
    if backup_core::cli::is_cli(&args) {
        std::process::exit(backup_core::cli::run(args));
    }
    claude_backup_lib::run();
}
