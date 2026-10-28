fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if let Err(e) = bilingua_cli::run(&args, &mut out) {
        eprintln!("bilingua: {e}");
        std::process::exit(1);
    }
}
