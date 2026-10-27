//! A sideways scroll for wide tables that stays inside the content. egui's `ScrollArea` springs
//! back past the ends on a trackpad (a macOS-style rubber band), which leaves a gap at the left or
//! right of a table. This keeps the offset between the two ends instead.

use egui::{Rect, Sense, Ui, UiBuilder, Vec2};

/// The sideways scroll of one table. Keep it in the screen's state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HScroll {
    /// How far the content is scrolled to the left, in points. Always within the content.
    pub offset: f32,
    /// The content's width as drawn last frame.
    pub content_width: f32,
}

/// Draws `add` in the space left in `ui`, shifted by the scroll offset and clipped to it. Horizontal
/// wheel or trackpad input over the space moves the offset, which is then kept between 0 and the
/// content's overflow. The content is laid out at the viewport's width, so a flexible column fills it.
pub fn show(ui: &mut Ui, state: &mut HScroll, add: impl FnOnce(&mut Ui)) {
    let viewport = ui.available_rect_before_wrap();
    if ui.rect_contains_pointer(viewport) {
        // Content moves right for a positive delta, so the offset goes down.
        let dx = ui.input(|i| i.smooth_scroll_delta.x);
        state.offset -= dx;
    }
    let overflow = (state.content_width - viewport.width()).max(0.0);
    state.offset = state.offset.clamp(0.0, overflow);

    let origin = viewport.min - Vec2::new(state.offset, 0.0);
    let content = Rect::from_min_size(origin, Vec2::new(viewport.width(), viewport.height()));
    let used = ui
        .scope_builder(UiBuilder::new().max_rect(content), |child| {
            child.set_clip_rect(viewport);
            add(child);
            child.min_rect()
        })
        .inner;
    state.content_width = used.width().max(viewport.width());
    ui.allocate_rect(viewport, Sense::hover());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The default scroll starts at the left end with nothing to scroll.
    fn default_is_at_the_left_end() {
        let s = HScroll::default();
        assert_eq!(s.offset, 0.0);
        assert_eq!(s.content_width, 0.0);
    }
}
