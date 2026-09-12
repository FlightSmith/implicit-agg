fn main() {
    let (code, output) = aircraft_cli::run(std::env::args().skip(1));
    print!("{output}");
    std::process::exit(code);
}
