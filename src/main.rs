fn main() {
    if let Err(err) = pcs::cli::run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}
