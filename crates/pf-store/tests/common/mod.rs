//! Helpers shared by the store integration tests.
#![allow(dead_code)]

use std::path::PathBuf;

use pf_model::{Change, Db, Record, RowKey};
use tempfile::{TempDir, tempdir};

/// A fresh temp directory and a database path inside it.
pub fn file_path() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("pf.sqlite");
    (dir, path)
}

/// One insert change per row of `db` (before = None).
pub fn inserts(db: &Db) -> Vec<Change> {
    db.all_records().into_iter().map(upsert).collect()
}

/// A change that writes `r` as its after-image.
pub fn upsert(r: Record) -> Change {
    Change {
        key: r.key(),
        before: None,
        after: Some(r),
    }
}

/// A change that removes the row with `key`.
pub fn remove(key: RowKey) -> Change {
    Change {
        key,
        before: None,
        after: None,
    }
}
