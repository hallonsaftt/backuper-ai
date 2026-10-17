//! Claude Backup — jezgro aplikacije (bez UI-a).
//!
//! Garancije:
//! * Claude fajlove samo ČITA. Ništa u Claude-u ne briše i ne menja.
//! * Kopira bajt po bajt i svaki fajl proverava SHA-256 zbirom — pri kopiranju i posle, iz gotovog backupa.
//! * Jedina izmena van backupa je „čuvanje istorije” (cleanupPeriodDays) — samo kad je korisnik izabere.
//! * Vraćanje samo DODAJE fajlove koji fale; postojeće ne prepisuje.

pub mod backup;
pub mod cli;
pub mod env;
pub mod i18n;
pub mod readable;
pub mod restore;
pub mod retention;
pub mod scan;
pub mod schedule;
pub mod util;
pub mod verify;

pub use backup::TOOL_VERSION;
