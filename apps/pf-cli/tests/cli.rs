//! CLI integration tests (T11, L4). Every test uses its own temp database and runs in
//! America/New_York time. Nothing here touches the app's real database.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use pf_engine::Engine;
use predicates::prelude::*;
use tempfile::TempDir;

/// Path of a file in the repo-level `fixtures/` directory.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// A fresh temp directory and a database path inside it. Keep the `TempDir` alive.
fn temp_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let db = dir.path().join("pf.sqlite");
    (dir, db)
}

/// A `pf-cli` command on `db`, in New York time, with `PF_DB` unset.
fn pf(db: &Path) -> Command {
    let mut c = Command::cargo_bin("pf-cli").expect("pf-cli binary");
    c.env("TZ", "America/New_York")
        .env_remove("PF_DB")
        .arg("--db")
        .arg(db);
    c
}

/// Imports `fixtures/{name}` into `db` with `import-md --yes`.
fn import_fixture(db: &Path, name: &str) {
    pf(db)
        .args(["import-md"])
        .arg(fixture(name))
        .arg("--yes")
        .assert()
        .success();
}

#[test]
/// `--help` prints the usage and exits 0.
fn cli_help_exit_0() {
    Command::cargo_bin("pf-cli")
        .expect("pf-cli binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
/// `--version` prints the program name and the workspace version, and exits 0.
fn cli_version_exit_0() {
    Command::cargo_bin("pf-cli")
        .expect("pf-cli binary")
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("pf-cli {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
/// An unknown command exits 2 and shows the usage on stderr.
fn cli_bad_command_exit_2() {
    let (_dir, db) = temp_db();
    pf(&db)
        .arg("frobnicate")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage"));
}

#[test]
/// `import-md` without `--yes` exits 2 and does not touch the data.
fn cli_import_md_requires_yes() {
    let (_dir, db) = temp_db();
    let file = fixture("world.md");
    pf(&db)
        .args(["import-md"])
        .arg(&file)
        .assert()
        .code(2)
        .stderr(format!(
            "import-md replaces ALL data in {}. Re-run with --yes to confirm.\n",
            file.display()
        ));
}

#[test]
/// Reality balances after importing the world fixture: external accounts are listed but left
/// out of the total (2051.95 + 10500 - 668.40 + 0).
fn cli_import_md_then_balances_text() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args(["balances", "--format", "text"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Checking (Chase): $2,051.95")
                .and(predicate::str::contains(
                    "External 401k (Fidelity) [external]: $1,000.00",
                ))
                .and(predicate::str::contains("Total: $11,883.55")),
        );
}

#[test]
/// `transactions checking -n 5` prints a 5-row report, newest first.
fn cli_transactions_n_5() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    let out = pf(&db)
        .args(["transactions", "checking", "-n", "5", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("UTF-8 output");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 9, "title, blank, header, rule and 5 rows");
    assert_eq!(lines[0], "Checking — Last 5 transactions");
    assert!(lines[4].starts_with("2026-03-12"), "line: {}", lines[4]);
}

#[test]
/// `--scenario "Plan A"` adds the scenario's rows: 2051.95 - 1500 = 551.95.
fn cli_scenario_flag() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args([
            "balance",
            "Checking",
            "--scenario",
            "Plan A",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"balance\" : 551.95"));
}

#[test]
/// An unknown scenario prints the error body on stdout and exits 1.
fn cli_unknown_scenario_exit_1() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args(["balances", "--scenario", "Nope"])
        .assert()
        .code(1)
        .stdout("Error: Scenario 'Nope' not found.\n");
}

#[test]
/// Markdown export, import into a second database, and export again give identical files.
fn cli_export_md_round_trip() {
    let (dir, db1) = temp_db();
    let db2 = dir.path().join("second.sqlite");
    import_fixture(&db1, "world.md");

    let first = dir.path().join("a.md");
    pf(&db1)
        .args(["export-md", "--out"])
        .arg(&first)
        .assert()
        .success();
    pf(&db2)
        .args(["import-md"])
        .arg(&first)
        .arg("--yes")
        .assert()
        .success();

    let second = dir.path().join("b.md");
    pf(&db2)
        .args(["export-md", "--out"])
        .arg(&second)
        .assert()
        .success();

    let a = fs::read_to_string(&first).expect("first export");
    let b = fs::read_to_string(&second).expect("second export");
    assert!(!a.is_empty());
    assert_eq!(a, b);
}

#[test]
/// A bank CSV needs saved column mappings. The world fixture has none for Checking.
fn cli_import_csv_requires_saved_mappings() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args(["import-csv", "Checking"])
        .arg(fixture("csv/bank_basic.csv"))
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "No saved column mappings for account 'Checking'. Please import via the UI first to set up column mappings.",
        ));
}

#[test]
/// `backup FOLDER` writes exactly one `PFBackup_*.md` file.
fn cli_backup_writes_file() {
    let (dir, db) = temp_db();
    import_fixture(&db, "world.md");
    let folder = dir.path().join("backups");
    fs::create_dir(&folder).expect("backup folder");
    pf(&db).args(["backup"]).arg(&folder).assert().success();

    let names: Vec<String> = fs::read_dir(&folder)
        .expect("read backup folder")
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.starts_with("PFBackup_") && n.ends_with(".md"))
        .collect();
    assert_eq!(names.len(), 1, "files: {names:?}");
}

#[test]
/// A running app (API port answering) blocks mutations: exit 1 with the "appears to be running"
/// message. The port comes from the database's `apiPort` setting.
fn cli_refuses_mutation_when_app_port_busy() {
    let (_dir, db) = temp_db();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let port = listener.local_addr().expect("local addr").port();
    {
        let mut engine = Engine::open(&db).expect("open engine");
        engine
            .set_setting_direct("apiPort", &port.to_string())
            .expect("store apiPort");
    }
    pf(&db)
        .args(["import-md"])
        .arg(fixture("world.md"))
        .arg("--yes")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("appears to be running"));
    drop(listener);
}

#[test]
/// The bundled sample export imports and balances run with exit 0.
fn cli_sample_data2_smoke() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "SampleData2.md");
    pf(&db).args(["balances"]).assert().success();
}

#[test]
/// `--db` wins over `PF_DB`; without `--db`, `PF_DB` is used.
fn cli_db_path_env_override() {
    let (dir, db) = temp_db();
    let env_db = dir.path().join("from-env.sqlite");

    pf(&db)
        .arg("db-path")
        .assert()
        .success()
        .stdout(format!("{}\n", db.display()));

    Command::cargo_bin("pf-cli")
        .expect("pf-cli binary")
        .env("PF_DB", &env_db)
        .arg("db-path")
        .assert()
        .success()
        .stdout(format!("{}\n", env_db.display()));
}

#[test]
/// An unknown account name exits 1 with the message on stderr.
fn cli_account_not_found_exit_1() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args(["transactions", "Nobody"])
        .assert()
        .code(1)
        .stderr("Account not found: Nobody\n");
}

#[test]
/// `scenarios` lists the live scenarios of the imported data.
fn cli_scenarios_lists_imported_scenarios() {
    let (_dir, db) = temp_db();
    import_fixture(&db, "world.md");
    pf(&db)
        .args(["scenarios"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Available Scenarios:\n- Plan A"));
}

#[test]
/// An unknown format name is a usage error (exit 2).
fn cli_bad_format_exit_2() {
    let (_dir, db) = temp_db();
    pf(&db)
        .args(["balances", "--format", "xml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage"));
}
