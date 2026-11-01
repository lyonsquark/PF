//! Runs one parsed command against the engine. Each command returns its exact stdout, or a
//! `Failure` with the exit code and the text for stdout and stderr.

use std::ffi::OsString;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use pf_core::api_format::{
    Format, balance_entries, balance_entry, error_body, format_balances, format_import_result,
    format_scenarios, format_transactions, resolve_scenario, transaction_lines,
};
use pf_engine::Engine;
use pf_engine::commands::backup::{export_markdown, import_markdown_replacing, perform_backup};
use pf_engine::commands::import::import_csv_with_saved_mappings;
use pf_model::{AccountId, ScenarioId};

use crate::args::{Command, USAGE};

/// The API port used when the database has no `apiPort` setting (Swift's fixed port).
pub const DEFAULT_API_PORT: u16 = 8989;

/// A command that did not succeed: the exit code and the text for each stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Process exit code (1 for runtime errors, 2 for refused usage).
    pub code: u8,
    /// Text for stdout, printed as is.
    pub stdout: String,
    /// Text for stderr, printed as is.
    pub stderr: String,
}

impl Failure {
    /// Exit 1 with `error: {msg}` on stderr.
    fn error(msg: impl std::fmt::Display) -> Failure {
        fail(1, format!("error: {msg}\n"))
    }
}

/// A failure with only stderr text.
fn fail(code: u8, stderr: String) -> Failure {
    Failure {
        code,
        stdout: String::new(),
        stderr,
    }
}

/// `s` followed by a newline.
fn line(s: &str) -> String {
    format!("{s}\n")
}

/// The database path: `--db` wins, then a non-empty `PF_DB`, then the default.
pub fn resolve_db_path(flag: Option<PathBuf>, env: Option<OsString>, default: PathBuf) -> PathBuf {
    if let Some(path) = flag {
        return path;
    }
    match env {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => default,
    }
}

/// Runs `cmd` against the database at `db`. `Ok` is the exact text for stdout.
pub fn run(cmd: &Command, db: &Path) -> Result<String, Failure> {
    match cmd {
        Command::Help => Ok(line(USAGE)),
        Command::Version => Ok(line(concat!("pf-cli ", env!("CARGO_PKG_VERSION")))),
        Command::DbPath => Ok(line(&db.to_string_lossy())),
        Command::Balances { scenario, format } => {
            let e = open(db)?;
            let sc = scenario_id(&e, scenario.as_deref(), *format)?;
            let entries = balance_entries(e.db(), sc);
            Ok(line(&format_balances(&entries, *format)))
        }
        Command::Balance {
            account,
            scenario,
            format,
        } => {
            let e = open(db)?;
            let (id, _) = find_account(&e, account)?;
            let sc = scenario_id(&e, scenario.as_deref(), *format)?;
            let entries: Vec<_> = balance_entry(e.db(), id, sc).into_iter().collect();
            Ok(line(&format_balances(&entries, *format)))
        }
        Command::Transactions {
            account,
            n,
            scenario,
            format,
        } => {
            let e = open(db)?;
            let (id, name) = find_account(&e, account)?;
            let sc = scenario_id(&e, scenario.as_deref(), *format)?;
            let lines = transaction_lines(e.db(), id, sc, *n);
            Ok(line(&format_transactions(&name, &lines, *format)))
        }
        Command::Scenarios { format } => {
            let e = open(db)?;
            Ok(line(&format_scenarios(e.db(), *format)))
        }
        Command::ExportMd { out } => {
            let e = open(db)?;
            let text = export_markdown(&e);
            match out {
                Some(path) => {
                    fs::write(path, text).map_err(|err| {
                        Failure::error(format!("cannot write {}: {err}", path.display()))
                    })?;
                    Ok(String::new())
                }
                None => Ok(text),
            }
        }
        Command::ImportMd { file, yes } => {
            if !yes {
                return Err(fail(
                    2,
                    line(&format!(
                        "import-md replaces ALL data in {}. Re-run with --yes to confirm.",
                        file.display()
                    )),
                ));
            }
            let mut e = open(db)?;
            refuse_if_app_running(&e)?;
            let text = read_file(file)?;
            let c = import_markdown_replacing(&mut e, &text, &mut |msg: &str| {
                log::info!("{msg}");
            })
            .map_err(Failure::error)?;
            Ok(line(&format!(
                "Imported {} accounts, {} transactions, {} categories, {} contexts, {} scenarios, {} obligations, {} budget items.",
                c.accounts,
                c.transactions,
                c.categories,
                c.contexts,
                c.scenarios,
                c.obligations,
                c.budget_items
            )))
        }
        Command::ImportCsv {
            account,
            file,
            format,
        } => {
            let mut e = open(db)?;
            refuse_if_app_running(&e)?;
            let (id, name) = find_account(&e, account)?;
            let csv = read_file(file)?;
            let r = import_csv_with_saved_mappings(&mut e, id, &csv).map_err(Failure::error)?;
            Ok(line(&format_import_result(
                &name,
                r.imported,
                r.skipped_duplicates,
                r.skipped_unmapped,
                *format,
            )))
        }
        Command::Backup { folder } => {
            let e = open(db)?;
            let written = perform_backup(&e, folder).map_err(Failure::error)?;
            Ok(line(&format!("Backup saved to: {}", written.display())))
        }
        Command::Purge => {
            open(db)?;
            Ok(String::new())
        }
    }
}

/// Opens the engine. Opening purges soft-deleted rows (R-PRG-1).
fn open(db: &Path) -> Result<Engine, Failure> {
    Engine::open(db).map_err(Failure::error)
}

/// The live account named `name` (any case), as its id and stored name.
fn find_account(e: &Engine, name: &str) -> Result<(AccountId, String), Failure> {
    e.db()
        .account_by_name_ci(name)
        .map(|a| (a.id, a.name.clone()))
        .ok_or_else(|| fail(1, line(&format!("Account not found: {name}"))))
}

/// The scenario for `name`. An unknown name prints the error body on stdout and exits 1.
fn scenario_id(e: &Engine, name: Option<&str>, f: Format) -> Result<Option<ScenarioId>, Failure> {
    resolve_scenario(e.db(), name).map_err(|msg| Failure {
        code: 1,
        stdout: line(&error_body(&msg, f)),
        stderr: String::new(),
    })
}

/// Reads a whole text file, naming it on failure.
fn read_file(path: &Path) -> Result<String, Failure> {
    fs::read_to_string(path)
        .map_err(|err| Failure::error(format!("cannot read {}: {err}", path.display())))
}

/// Refuses to change data while the app's API answers on its port (the app and the CLI would
/// allocate ids from the same file). The port is the `apiPort` setting, else 8989.
fn refuse_if_app_running(e: &Engine) -> Result<(), Failure> {
    let port = e
        .setting("apiPort")
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_API_PORT);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
        return Err(fail(
            1,
            line(&format!(
                "The PF app appears to be running (API on port {port}). Quit it first, or use the HTTP API."
            )),
        ));
    }
    Ok(())
}
