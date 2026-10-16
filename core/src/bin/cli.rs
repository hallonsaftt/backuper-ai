fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::process::exit(backup_core::cli::run(args));
}
