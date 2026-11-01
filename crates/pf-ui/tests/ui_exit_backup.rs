//! L5 UI tests for the backup made when the app exits (R-MD-8). Headless (egui_kittest): a close request
//! is sent as a viewport event, and Cmd-Q, which skips it, is the direct call to `exit_backup`.

mod common;

use std::path::Path;

use egui::{ViewportCommand, ViewportEvent, ViewportId};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pf_engine::commands::{backup, settings};
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
use pf_ui::screen::Screen;
use tempfile::TempDir;

use common::{engine_with_fixture, harness};

/// The backups in `dir`.
fn backups(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .expect("read the folder")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("PFBackup_"))
        .count()
}

/// An app with the fake dialogs, and its backup folder set to `folder` when given.
fn app_with(folder: Option<&Path>, files: &FakeDialogs) -> PfApp {
    let mut engine = engine_with_fixture();
    if let Some(folder) = folder {
        backup::set_backup_folder(&mut engine, folder).expect("set the folder");
    }
    PfApp::new_for_test_with_files(engine, files.clone())
}

/// Sends a window close request and returns whether the app kept the window open.
fn close_window(h: &mut Harness<'_>) -> bool {
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.step();
    h.output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&ViewportCommand::CancelClose))
}

#[test]
/// R-MD-8: it is on unless turned off.
fn exit_backup_is_on_by_default() {
    let mut e = engine_with_fixture();
    assert!(settings::backup_on_exit(&e));
    settings::set_backup_on_exit(&mut e, false).expect("turn off");
    assert!(!settings::backup_on_exit(&e));
    settings::set_backup_on_exit(&mut e, true).expect("turn on");
    assert!(settings::backup_on_exit(&e));
}

#[test]
/// R-MD-8: closing the window writes one backup, without asking, and the window closes.
fn closing_the_window_backs_up() {
    let dir = TempDir::new().expect("temp folder");
    let files = FakeDialogs::default();
    let mut app = app_with(Some(dir.path()), &files);
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!close_window(&mut h), "the window closes");
    }
    assert_eq!(backups(dir.path()), 1);
    assert_eq!(
        files.calls(),
        ["message:Backup Saved"],
        "no folder prompt, one message"
    );
    app.exit_backup();
    assert_eq!(backups(dir.path()), 1, "on_exit does not back up again");
}

#[test]
/// R-MD-8: Cmd-Q skips the close request; `on_exit` still writes the backup, once.
fn quitting_backs_up() {
    let dir = TempDir::new().expect("temp folder");
    let files = FakeDialogs::default();
    let mut app = app_with(Some(dir.path()), &files);
    app.exit_backup();
    assert_eq!(backups(dir.path()), 1);
    app.exit_backup();
    assert_eq!(backups(dir.path()), 1, "only once");
}

#[test]
/// R-MD-8: with no folder set it asks, as Cmd-S does, and remembers the answer.
fn no_folder_asks_as_cmd_s_does() {
    let dir = TempDir::new().expect("temp folder");
    let files = FakeDialogs::default();
    files.push_answer(Some(dir.path().to_path_buf()));
    let mut app = app_with(None, &files);
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!close_window(&mut h));
    }
    assert_eq!(
        files.calls(),
        ["pick_folder:Set Backup Folder", "message:Backup Saved"]
    );
    assert_eq!(backups(dir.path()), 1);
    let e = shared.lock().expect("lock");
    assert_eq!(backup::backup_folder(&e).as_deref(), Some(dir.path()));
}

#[test]
/// R-MD-8: cancelling that question does not stop the app from closing, and it is asked only once.
fn cancelling_the_question_still_closes() {
    let files = FakeDialogs::default();
    files.push_answer(None);
    let mut app = app_with(None, &files);
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!close_window(&mut h), "the window closes");
    }
    app.exit_backup();
    assert_eq!(files.calls().len(), 1);
}

#[test]
/// R-MD-8: turned off, nothing is written or asked, on either way out.
fn turned_off_does_nothing() {
    let files = FakeDialogs::default();
    let mut engine = engine_with_fixture();
    settings::set_backup_on_exit(&mut engine, false).expect("turn off");
    let mut app = PfApp::new_for_test_with_files(engine, files.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!close_window(&mut h));
    }
    app.exit_backup();
    assert!(files.calls().is_empty());
}

#[test]
/// R-MD-8: a failure keeps the window open once to show the alert. Closing again goes ahead.
fn failure_keeps_the_window_open_once() {
    let dir = TempDir::new().expect("temp folder");
    let gone = dir.path().join("gone");
    let files = FakeDialogs::default();
    let mut app = app_with(Some(&gone), &files);
    let mut h = harness(&mut app);
    h.run();
    assert!(close_window(&mut h), "the first failure keeps it open");
    h.run();
    assert!(
        h.query_by_label("Backup Error").is_some(),
        "the alert shows"
    );
    assert!(!close_window(&mut h), "the second close goes ahead");
}

#[test]
/// R-MD-8: with nothing changed since the newest backup, an exit writes nothing and says nothing.
fn unchanged_data_writes_nothing() {
    let dir = TempDir::new().expect("temp folder");
    let files = FakeDialogs::default();
    let mut engine = engine_with_fixture();
    backup::set_backup_folder(&mut engine, dir.path()).expect("set the folder");
    backup::perform_backup(&engine, dir.path()).expect("an earlier backup");
    let mut app = PfApp::new_for_test_with_files(engine, files.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!close_window(&mut h), "the window closes");
    }
    assert_eq!(backups(dir.path()), 1, "no new file");
    assert!(files.calls().is_empty(), "no message");
}

#[test]
/// R-MD-8: a change since the newest backup is backed up, and Cmd-Q's path says so too.
fn changed_data_writes_and_says_so() {
    let dir = TempDir::new().expect("temp folder");
    let files = FakeDialogs::default();
    let mut engine = engine_with_fixture();
    backup::set_backup_folder(&mut engine, dir.path()).expect("set the folder");
    std::fs::write(
        dir.path().join("PFBackup_2026-01-01_00-00-00.md"),
        "older data",
    )
    .expect("an earlier backup");
    let mut app = PfApp::new_for_test_with_files(engine, files.clone());
    app.exit_backup();
    assert_eq!(backups(dir.path()), 2);
    assert_eq!(files.calls(), ["message:Backup Saved"]);
}

#[test]
/// R-MD-8: on Cmd-Q's path the app cannot stay open, so a failure is shown in the small window.
fn quitting_reports_a_failure() {
    let dir = TempDir::new().expect("temp folder");
    let gone = dir.path().join("gone");
    let files = FakeDialogs::default();
    let mut app = app_with(Some(&gone), &files);
    app.exit_backup();
    assert_eq!(files.calls(), ["message:Backup Error"]);
}

#[test]
/// R-MD-8: only the newest backup counts: going back to older data is a change. `X-2.md` is newer than
/// `X.md`, though it sorts before it.
fn only_the_newest_backup_counts() {
    let dir = TempDir::new().expect("temp folder");
    let engine = engine_with_fixture();
    let current = backup::export_markdown(&engine);
    let older = dir.path().join("PFBackup_2026-01-01_00-00-00.md");
    let newest = dir.path().join("PFBackup_2026-02-02_00-00-00.md");
    std::fs::write(&older, &current).expect("write");
    std::fs::write(&newest, "other data").expect("write");
    assert!(!backup::backup_is_current(&engine, dir.path()));
    std::fs::write(&newest, &current).expect("write");
    assert!(backup::backup_is_current(&engine, dir.path()));
    let same_second = dir.path().join("PFBackup_2026-02-02_00-00-00-2.md");
    std::fs::write(&same_second, "other data").expect("write");
    assert!(!backup::backup_is_current(&engine, dir.path()));
    // Other files and an unfinished backup do not count.
    std::fs::remove_file(&same_second).expect("remove");
    std::fs::write(dir.path().join("notes.md"), "x").expect("write");
    std::fs::write(dir.path().join("PFBackup_2027-01-01_00-00-00.md.tmp"), "x").expect("write");
    assert!(backup::backup_is_current(&engine, dir.path()));
}

#[test]
/// R-MD-8: the Settings checkbox starts on and stores the choice.
fn settings_checkbox_turns_it_off() {
    let files = FakeDialogs::default();
    let mut app = app_with(None, &files);
    app.set_screen(Screen::Settings);
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Back up automatically when the app exits")
            .click();
        h.run();
    }
    assert!(!settings::backup_on_exit(&shared.lock().expect("lock")));
}
