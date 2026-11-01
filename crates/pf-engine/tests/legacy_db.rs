//! The database moves from its old name (`PFRust.sqlite`) to the new one on the first start after the
//! app's rename, and is never put at risk doing so. Every path here is in a temp folder.

use std::net::TcpListener;
use std::path::{Path, PathBuf};

use pf_engine::Engine;
use pf_model::AccountType;
use pf_model::testing::TestDb;
use tempfile::TempDir;

/// A port nothing is listening on.
fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind a free port");
    listener.local_addr().expect("its address").port()
}

/// A temp folder with a real database under the old name, holding the account "Savings".
fn folder_with_old_db() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("temp folder");
    let old = dir.path().join("PFRust.sqlite");
    let new = dir.path().join("PF.sqlite");
    let mut t = TestDb::new();
    t.account("Savings", AccountType::Savings);
    let mut e = Engine::open(&old).expect("open the old file");
    e.replace_db(t.build()).expect("write the old file");
    drop(e);
    (dir, old, new)
}

/// The names of the accounts in the database at `path`.
fn accounts(path: &Path) -> Vec<String> {
    let e = Engine::open(path).expect("open the database");
    e.db().live_accounts().map(|a| a.name.clone()).collect()
}

#[test]
/// The data moves with the file, and the old name is gone.
fn old_database_moves_with_its_data() {
    let (_dir, old, new) = folder_with_old_db();
    assert!(Engine::move_legacy_db(&new, free_port()).expect("move"));
    assert!(!old.exists());
    assert_eq!(accounts(&new), ["Savings"]);
}

#[test]
/// The `-wal` and `-shm` files go along, since unsaved changes can live in them.
fn side_files_move_too() {
    let (_dir, old, new) = folder_with_old_db();
    let side = |base: &Path, suffix: &str| {
        let mut name = base.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    std::fs::write(side(&old, "-wal"), "wal").expect("write");
    std::fs::write(side(&old, "-shm"), "shm").expect("write");
    assert!(Engine::move_legacy_db(&new, free_port()).expect("move"));
    assert_eq!(std::fs::read(side(&new, "-wal")).expect("wal"), b"wal");
    assert_eq!(std::fs::read(side(&new, "-shm")).expect("shm"), b"shm");
    assert!(!side(&old, "-wal").exists());
    assert!(!side(&old, "-shm").exists());
}

#[test]
/// While an app is running, nothing moves and the caller gets an error, not an empty new database.
fn refuses_while_an_app_is_running() {
    let (_dir, old, new) = folder_with_old_db();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("its address").port();
    let err = Engine::move_legacy_db(&new, port).expect_err("refused");
    assert!(err.to_string().contains("Quit it"), "{err}");
    assert!(old.exists());
    assert!(!new.exists());
}

#[test]
/// A database already at the new name is never replaced.
fn existing_new_database_is_kept() {
    let (_dir, old, new) = folder_with_old_db();
    let mut t = TestDb::new();
    t.account("Current", AccountType::Checking);
    let mut e = Engine::open(&new).expect("open the new file");
    e.replace_db(t.build()).expect("write the new file");
    drop(e);
    assert!(!Engine::move_legacy_db(&new, free_port()).expect("nothing to do"));
    assert!(old.exists());
    assert_eq!(accounts(&new), ["Current"]);
}

#[test]
/// With no old database there is nothing to do, even while an app is running.
fn no_old_database_is_fine() {
    let dir = TempDir::new().expect("temp folder");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("its address").port();
    let new = dir.path().join("PF.sqlite");
    assert!(!Engine::move_legacy_db(&new, port).expect("nothing to do"));
}
