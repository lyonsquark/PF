//! Build automation. `rules` and `ci` are implemented in T00b.

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    eprintln!("xtask: unknown task '{task}' (implemented in T00b)");
    std::process::exit(2);
}
