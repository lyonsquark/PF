//! Native file dialogs and "open in default app", behind a trait so tests can fake them.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Native dialogs and opening files. The UI calls these on the UI thread only.
pub trait FileDialogs {
    /// Asks the user for one file. `extensions` may be empty (any file).
    fn pick_file(&self, title: &str, extensions: &[&str]) -> Option<PathBuf>;
    /// Asks the user for a folder.
    fn pick_folder(&self, title: &str) -> Option<PathBuf>;
    /// Asks where to save a file.
    fn save_file(&self, title: &str, default_name: &str, extensions: &[&str]) -> Option<PathBuf>;
    /// Opens a file with the OS default app (`open <path>` on macOS).
    fn open_path(&self, path: &Path) -> Result<(), String>;
    /// Shows a small native message with an OK button, and waits for it.
    fn message(&self, title: &str, text: &str);
}

/// The rfd-backed implementation used by the desktop app.
pub struct RfdDialogs;

impl FileDialogs for RfdDialogs {
    fn pick_file(&self, title: &str, extensions: &[&str]) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if !extensions.is_empty() {
            dialog = dialog.add_filter("Files", extensions);
        }
        dialog.pick_file()
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }

    fn save_file(&self, title: &str, default_name: &str, extensions: &[&str]) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(default_name);
        if !extensions.is_empty() {
            dialog = dialog.add_filter("Files", extensions);
        }
        dialog.save_file()
    }

    fn message(&self, title: &str, text: &str) {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title(title)
            .set_description(text)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }

    fn open_path(&self, path: &Path) -> Result<(), String> {
        let status = std::process::Command::new("open")
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("open exited with {status}"))
        }
    }
}

/// What the fake dialogs have been asked and answered.
#[derive(Default)]
pub struct FakeState {
    /// Answers for `pick_file`, `pick_folder` and `save_file`, in call order.
    pub picks: std::collections::VecDeque<Option<PathBuf>>,
    /// Paths passed to `open_path`.
    pub opened: Vec<PathBuf>,
    /// One entry per call, e.g. "pick_folder:Set Backup Folder".
    pub calls: Vec<String>,
}

/// Test double. Clones share one state, so a test keeps a clone to inspect the app's calls.
#[derive(Clone, Default)]
pub struct FakeDialogs {
    inner: Rc<RefCell<FakeState>>,
}

impl FakeDialogs {
    /// Queues the answer to the next pick. `None` means the user cancelled.
    pub fn push_answer(&self, p: Option<PathBuf>) {
        self.inner.borrow_mut().picks.push_back(p);
    }

    /// Every path opened so far.
    pub fn opened(&self) -> Vec<PathBuf> {
        self.inner.borrow().opened.clone()
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<String> {
        self.inner.borrow().calls.clone()
    }

    /// Records `call` and returns the next queued answer (`None` when the queue is empty).
    fn answer(&self, call: String) -> Option<PathBuf> {
        let mut state = self.inner.borrow_mut();
        state.calls.push(call);
        state.picks.pop_front().flatten()
    }
}

impl FileDialogs for FakeDialogs {
    fn pick_file(&self, title: &str, _extensions: &[&str]) -> Option<PathBuf> {
        self.answer(format!("pick_file:{title}"))
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        self.answer(format!("pick_folder:{title}"))
    }

    fn save_file(&self, title: &str, _default_name: &str, _extensions: &[&str]) -> Option<PathBuf> {
        self.answer(format!("save_file:{title}"))
    }

    fn message(&self, title: &str, _text: &str) {
        self.inner
            .borrow_mut()
            .calls
            .push(format!("message:{title}"));
    }

    fn open_path(&self, path: &Path) -> Result<(), String> {
        let mut state = self.inner.borrow_mut();
        state.calls.push(format!("open_path:{}", path.display()));
        state.opened.push(path.to_path_buf());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: file dialog fake (T13 spec). Answers come back in order, then None; calls recorded.
    fn fake_dialogs_returns_queued_answers_in_order() {
        let fake = FakeDialogs::default();
        fake.push_answer(Some(PathBuf::from("/a")));
        fake.push_answer(Some(PathBuf::from("/b")));
        assert_eq!(fake.pick_folder("x"), Some(PathBuf::from("/a")));
        assert_eq!(fake.pick_file("y", &["md"]), Some(PathBuf::from("/b")));
        assert_eq!(fake.pick_folder("z"), None);
        assert_eq!(
            fake.calls(),
            vec!["pick_folder:x", "pick_file:y", "pick_folder:z"]
        );
    }

    #[test]
    /// Covers: file dialog fake (T13 spec). A queued `None` is a cancel; clones share state.
    fn fake_dialogs_clones_share_state_and_record_opens() {
        let fake = FakeDialogs::default();
        let shared = fake.clone();
        shared.push_answer(None);
        assert_eq!(fake.pick_folder("cancel me"), None);
        assert!(fake.open_path(Path::new("/tmp/x.md")).is_ok());
        assert_eq!(shared.opened(), vec![PathBuf::from("/tmp/x.md")]);
        assert_eq!(shared.calls().len(), 2);
    }
}
