//! Bottom-centre toast messages: the backup path (R-MD-7) and the Undo/Redo notices. Pure state
//! (`Toasts`, testable without egui) plus one draw function.

use std::time::Duration;

use egui::{Align2, Area, Frame, Id, Order, RichText, Ui, vec2};

use crate::widgets::colors;

/// Seconds a plain info toast stays on screen.
const DEFAULT_SECS: f64 = 2.5;

/// How a toast looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Normal text colour.
    Info,
    /// Green text.
    Success,
}

/// The toast on screen. `shown_at` is set by the first `tick` after it appears.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    /// The message text.
    pub text: String,
    /// Colour style.
    pub kind: ToastKind,
    /// How long it stays, in seconds.
    pub secs: f64,
    shown_at: Option<f64>,
}

/// At most one toast. A new one replaces the current one.
#[derive(Debug, Default)]
pub struct Toasts {
    current: Option<Toast>,
}

impl Toasts {
    /// An info toast for 2.5 seconds.
    pub fn info(&mut self, text: impl Into<String>) {
        self.show_for(text, ToastKind::Info, DEFAULT_SECS);
    }

    /// A green toast for 2.5 seconds.
    pub fn success(&mut self, text: impl Into<String>) {
        self.show_for(text, ToastKind::Success, DEFAULT_SECS);
    }

    /// Shows `text` for `secs` seconds, replacing any current toast.
    pub fn show_for(&mut self, text: impl Into<String>, kind: ToastKind, secs: f64) {
        self.current = Some(Toast {
            text: text.into(),
            kind,
            secs,
            shown_at: None,
        });
    }

    /// The text of the toast on screen, for tests.
    pub fn current_text(&self) -> Option<&str> {
        self.current.as_ref().map(|t| t.text.as_str())
    }

    /// Pure expiry logic: the first call records `now` as the start; the toast is cleared once
    /// `now - start >= secs`.
    pub fn tick(&mut self, now: f64) {
        let Some(toast) = self.current.as_mut() else {
            return;
        };
        let start = *toast.shown_at.get_or_insert(now);
        if now - start >= toast.secs {
            self.current = None;
        }
    }
}

/// Draws the current toast at the bottom centre and asks for a repaint when it expires.
pub fn draw(toasts: &mut Toasts, ui: &mut Ui) {
    let now = ui.input(|i| i.time);
    toasts.tick(now);
    let Some(toast) = toasts.current.as_ref() else {
        return;
    };
    let text_color = match toast.kind {
        ToastKind::Info => ui.visuals().text_color(),
        ToastKind::Success => colors::GREEN,
    };
    let text = RichText::new(toast.text.as_str()).color(text_color);
    let remaining = toast.secs - (now - toast.shown_at.unwrap_or(now));
    Area::new(Id::unique("toast"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -20.0))
        .show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(text);
            });
        });
    ui.ctx()
        .request_repaint_after(Duration::from_secs_f64(remaining.max(0.0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: UI shell toasts (T13 spec).
    fn toasts_expire_after_duration() {
        let mut t = Toasts::default();
        t.info("hello");
        t.tick(0.0);
        assert_eq!(t.current_text(), Some("hello"));
        t.tick(2.4);
        assert_eq!(t.current_text(), Some("hello"));
        t.tick(2.6);
        assert_eq!(t.current_text(), None);
    }

    #[test]
    /// Covers: UI shell toasts (T13 spec).
    fn toasts_new_replaces_old() {
        let mut t = Toasts::default();
        t.info("first");
        t.tick(0.0);
        t.info("second");
        assert_eq!(t.current_text(), Some("second"));
        // The new toast starts its own clock.
        t.tick(2.0);
        assert_eq!(t.current_text(), Some("second"));
    }

    #[test]
    /// Covers: UI shell toasts (T13 spec). Success uses the success kind and 2.5 s.
    fn success_toast_uses_success_kind() {
        let mut t = Toasts::default();
        t.success("saved");
        let toast = t.current.as_ref().expect("toast present");
        assert_eq!(toast.kind, ToastKind::Success);
        assert!((toast.secs - 2.5).abs() < f64::EPSILON);
    }
}
