//! Hand-rolled argument parser for `pf-cli` (no clap). The usage text below is the spec.

use std::path::PathBuf;

use pf_core::api_format::{Format, parse_format};

/// The usage text, printed by `help`, `--help` and after a usage error.
pub const USAGE: &str = "Usage: pf-cli [--db PATH] <command> [options]

Commands:
  db-path                                   print the database path
  balances [--scenario NAME] [--format F]   all account balances
  balance ACCOUNT [--scenario NAME] [--format F]
  transactions ACCOUNT [-n N] [--scenario NAME] [--format F]   (N default 10)
  scenarios [--format F]
  export-md [--out FILE]                    Markdown backup to stdout or FILE
  import-md FILE --yes                      REPLACE all data with a Markdown backup
  import-csv ACCOUNT FILE [--format F]      import using the account's saved column mappings
  backup FOLDER                             write PFBackup_<timestamp>.md into FOLDER
  purge                                     open the DB (which purges soft-deleted rows) and exit
  help | --help | -h
  --version                                 print the version

F = json | text | markdown (default text). --db defaults to $PF_DB, then the app database.";

/// The parsed command line: the global `--db` option and one command.
#[derive(Debug, Clone, PartialEq)]
pub struct Cli {
    /// `--db PATH`, if given.
    pub db: Option<PathBuf>,
    /// The command to run.
    pub command: Command,
}

/// One CLI command with its options.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Print the database path.
    DbPath,
    /// Balances of all accounts.
    Balances {
        /// Scenario name; `None` is Reality.
        scenario: Option<String>,
        /// Output format.
        format: Format,
    },
    /// Balance of one account.
    Balance {
        /// Account name, matched case-insensitively.
        account: String,
        /// Scenario name; `None` is Reality.
        scenario: Option<String>,
        /// Output format.
        format: Format,
    },
    /// The newest transactions of one account.
    Transactions {
        /// Account name, matched case-insensitively.
        account: String,
        /// How many rows to print.
        n: usize,
        /// Scenario name; `None` is Reality.
        scenario: Option<String>,
        /// Output format.
        format: Format,
    },
    /// The scenario list.
    Scenarios {
        /// Output format.
        format: Format,
    },
    /// Markdown backup to stdout, or to a file.
    ExportMd {
        /// Destination file; `None` writes to stdout.
        out: Option<PathBuf>,
    },
    /// Replace all data with a Markdown backup.
    ImportMd {
        /// The backup file.
        file: PathBuf,
        /// Must be set, or the command refuses.
        yes: bool,
    },
    /// Import a bank CSV with the account's saved column mappings.
    ImportCsv {
        /// Target account name, matched case-insensitively.
        account: String,
        /// The CSV file.
        file: PathBuf,
        /// Output format of the summary.
        format: Format,
    },
    /// Write a Markdown backup file into a folder.
    Backup {
        /// Destination folder.
        folder: PathBuf,
    },
    /// Open the database (which purges soft-deleted rows) and exit.
    Purge,
    /// Print the usage.
    Help,
    /// `--version`: print the program name and version.
    Version,
}

/// Options and arguments collected while scanning one command's arguments.
#[derive(Default)]
struct Scanned {
    /// Arguments that are not options.
    positional: Vec<String>,
    /// `-h` or `--help` was given.
    help: bool,
    /// `--scenario NAME`.
    scenario: Option<String>,
    /// `--format F`.
    format: Option<Format>,
    /// `-n N`.
    n: Option<usize>,
    /// `--out FILE`.
    out: Option<PathBuf>,
    /// `--yes`.
    yes: bool,
}

/// Parses the arguments after the program name. `Err(msg)` means a usage error (exit 2).
pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut db = None;
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        match arg.as_str() {
            "--db" => {
                db = Some(PathBuf::from(value(args, i, "--db")?));
                i += 2;
            }
            "--help" | "-h" => {
                return Ok(Cli {
                    db,
                    command: Command::Help,
                });
            }
            "--version" => {
                return Ok(Cli {
                    db,
                    command: Command::Version,
                });
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown option '{other}'."));
            }
            _ => break,
        }
    }
    let Some(name) = args.get(i) else {
        return Err("No command given.".to_string());
    };
    let command = parse_command(name, &args[i + 1..])?;
    Ok(Cli { db, command })
}

/// The value after the option at `i`, or an error when there is none.
fn value<'a>(args: &'a [String], i: usize, flag: &str) -> Result<&'a str, String> {
    args.get(i + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} needs a value."))
}

/// Builds the command `name` from its own arguments.
fn parse_command(name: &str, rest: &[String]) -> Result<Command, String> {
    let allowed: &[&str] = match name {
        "db-path" | "purge" | "help" | "backup" => &[],
        "balances" | "balance" => &["--scenario", "--format"],
        "scenarios" => &["--format"],
        "transactions" => &["-n", "--scenario", "--format"],
        "export-md" => &["--out"],
        "import-md" => &["--yes"],
        "import-csv" => &["--format"],
        other => return Err(format!("Unknown command '{other}'.")),
    };
    let scanned = scan(name, rest, allowed)?;
    if scanned.help {
        return Ok(Command::Help);
    }
    let format = scanned.format.unwrap_or(Format::Text);
    let mut pos = scanned.positional.iter().cloned();
    let command = match name {
        "help" => Command::Help,
        "db-path" => Command::DbPath,
        "purge" => Command::Purge,
        "balances" => Command::Balances {
            scenario: scanned.scenario.clone(),
            format,
        },
        "balance" => Command::Balance {
            account: one(&mut pos, name, "ACCOUNT")?,
            scenario: scanned.scenario.clone(),
            format,
        },
        "transactions" => Command::Transactions {
            account: one(&mut pos, name, "ACCOUNT")?,
            n: scanned.n.unwrap_or(10),
            scenario: scanned.scenario.clone(),
            format,
        },
        "scenarios" => Command::Scenarios { format },
        "export-md" => Command::ExportMd {
            out: scanned.out.clone(),
        },
        "import-md" => Command::ImportMd {
            file: PathBuf::from(one(&mut pos, name, "FILE")?),
            yes: scanned.yes,
        },
        "import-csv" => Command::ImportCsv {
            account: one(&mut pos, name, "ACCOUNT")?,
            file: PathBuf::from(one(&mut pos, name, "FILE")?),
            format,
        },
        "backup" => Command::Backup {
            folder: PathBuf::from(one(&mut pos, name, "FOLDER")?),
        },
        _ => return Err(format!("Unknown command '{name}'.")),
    };
    if pos.next().is_some() {
        return Err(format!("Too many arguments for {name}."));
    }
    Ok(command)
}

/// Takes the next positional argument, or fails naming it.
fn one(pos: &mut impl Iterator<Item = String>, cmd: &str, what: &str) -> Result<String, String> {
    pos.next().ok_or_else(|| format!("{cmd} needs {what}."))
}

/// Scans `rest` for the options in `allowed`. Any other option is an error.
fn scan(cmd: &str, rest: &[String], allowed: &[&str]) -> Result<Scanned, String> {
    let mut out = Scanned::default();
    let mut i = 0;
    while let Some(arg) = rest.get(i) {
        let a = arg.as_str();
        if a == "--help" || a == "-h" {
            out.help = true;
            return Ok(out);
        }
        if !a.starts_with('-') || a == "-" {
            out.positional.push(a.to_string());
            i += 1;
            continue;
        }
        if !allowed.contains(&a) {
            return Err(format!("Unknown option '{a}' for {cmd}."));
        }
        if a == "--yes" {
            out.yes = true;
            i += 1;
            continue;
        }
        match a {
            "--scenario" => out.scenario = Some(value(rest, i, a)?.to_string()),
            "--format" => out.format = Some(format_value(value(rest, i, a)?)?),
            "-n" => {
                let raw = value(rest, i, a)?;
                let n = raw
                    .parse::<usize>()
                    .map_err(|_| format!("-n needs a whole number, got '{raw}'."))?;
                out.n = Some(n);
            }
            "--out" => out.out = Some(PathBuf::from(value(rest, i, a)?)),
            _ => return Err(format!("Unknown option '{a}' for {cmd}.")),
        }
        i += 2;
    }
    Ok(out)
}

/// `json`, `text`, `markdown` or `md`, in any case. Anything else is an error.
fn format_value(s: &str) -> Result<Format, String> {
    match s.to_lowercase().as_str() {
        "json" | "text" | "markdown" | "md" => Ok(parse_format(s)),
        _ => Err(format!("Unknown format '{s}'. Use json, text or markdown.")),
    }
}
