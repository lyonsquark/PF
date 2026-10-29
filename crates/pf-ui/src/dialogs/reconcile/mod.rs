//! The Reconcile dialog (T30): load a statement file, review the pairing, finish. New feature with no
//! Swift source; `docs/reconciliation-plan.md` and `tasks/T30-reconcile-dialog.md` are the truth.
//! The Load step is `load.rs`, the Review step `review.rs`.

mod load;
mod review;

use egui::{Id, Modal, Ui};
use pf_core::reconcile::Statement;
use pf_model::{AccountId, Db};

use super::Outcome;
use crate::app::UiCtx;

pub use load::LoadState;
pub use review::Review;

/// Dialog width in points (fits the 800 pt window of the drill-down helpers).
const WIDTH: f32 = 760.0;

/// The dialog's state. It lives in `Dialog::Reconcile` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileForm {
    /// The account being reconciled.
    pub account: AccountId,
    /// The Load step's fields.
    pub load: LoadState,
    /// The Review step; `Some` once a statement is loaded.
    pub review: Option<Review>,
}

impl ReconcileForm {
    /// Starts at the Load step.
    pub fn new_load(account: AccountId) -> Self {
        Self {
            account,
            load: LoadState::default(),
            review: None,
        }
    }

    /// Starts at the Review step with `statement` (used by the API banner, T31).
    pub fn with_statement(
        db: &Db,
        account: AccountId,
        statement: Statement,
    ) -> Result<Self, String> {
        Ok(Self {
            account,
            load: LoadState::default(),
            review: Some(Review::new(db, account, statement, None)?),
        })
    }
}

/// What a step asks the dialog to do after it is drawn.
pub(super) enum Action {
    None,
    Cancel,
    Loaded(Box<Statement>),
    Done,
}

/// Draws the dialog for the current step.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ReconcileForm) -> Outcome {
    let mut action = Action::None;
    let account = form.account;
    let response = Modal::new(Id::unique(("reconcile",))).show(ui.ctx(), |ui| {
        ui.set_width(WIDTH);
        ui.heading("Reconcile Statement");
        match &mut form.review {
            None => load::show(cx, ui, account, &mut form.load, &mut action),
            Some(review) => review::show(cx, ui, account, review, &mut action),
        }
    });
    match action {
        Action::None => {}
        Action::Cancel | Action::Done => return Outcome::Close,
        Action::Loaded(statement) => match Review::new(cx.engine.db(), account, *statement, None) {
            Ok(review) => form.review = Some(review),
            Err(e) => form.load.error = Some(e),
        },
    }
    if response.should_close() {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}
