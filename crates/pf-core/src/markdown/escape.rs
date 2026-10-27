//! Pipe escaping for Markdown table cells (R-MD-4).

/// The fullwidth vertical bar `｜` (U+FF5C) that stands in for `|` inside a table cell.
const PIPE_SUBSTITUTE: char = '\u{FF5C}';

/// R-MD-4: replaces every `|` with `｜` (U+FF5C) so the value fits in a table cell.
pub fn esc(value: &str) -> String {
    value.replace('|', &PIPE_SUBSTITUTE.to_string())
}

/// R-MD-4: restores `|` from `｜`; the inverse of [`esc`] for strings that contain no `｜`.
pub fn unesc(value: &str) -> String {
    value.replace(PIPE_SUBSTITUTE, "|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Covers: R-MD-4
    #[test]
    fn r_md_4_esc_round_trips_pipes() {
        assert_eq!(esc("a|b"), "a｜b");
        assert_eq!(unesc("a｜b"), "a|b");
        for s in ["", "plain", "a|b|c", "||", "x | y"] {
            assert_eq!(unesc(&esc(s)), s);
        }
    }

    proptest! {
        /// Covers: R-MD-4
        #[test]
        fn r_md_4_escape_property(s in "\\PC*".prop_filter("no fullwidth bar", |s| !s.contains(PIPE_SUBSTITUTE))) {
            prop_assert_eq!(unesc(&esc(&s)), s.clone());
            prop_assert!(!esc(&s).contains('|'));
        }
    }
}
