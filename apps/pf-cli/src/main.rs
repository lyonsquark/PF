//! `pf-cli`: a headless front end to the PF engine (T11). Parses the arguments, runs one command
//! and prints its result. Logs go to stderr through `env_logger`.

mod args;
mod commands;

use std::io::Write;
use std::process::ExitCode;

use pf_engine::Engine;

fn main() -> ExitCode {
    env_logger::init();
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let cli = match args::parse_args(&argv) {
        Ok(cli) => cli,
        Err(msg) => {
            eprint!("{msg}\n{}\n", args::USAGE);
            return ExitCode::from(2);
        }
    };
    let db =
        commands::resolve_db_path(cli.db, std::env::var_os("PF_DB"), Engine::default_db_path());
    match commands::run(&cli.command, &db) {
        Ok(out) => {
            // A closed pipe is not an error for a CLI that prints its result.
            let _ = std::io::stdout().write_all(out.as_bytes());
            ExitCode::SUCCESS
        }
        Err(f) => {
            let _ = std::io::stdout().write_all(f.stdout.as_bytes());
            let _ = std::io::stderr().write_all(f.stderr.as_bytes());
            ExitCode::from(f.code)
        }
    }
}
