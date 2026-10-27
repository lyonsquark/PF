//! Generated context names for new obligations (R-OBL-4).

/// R-OBL-4: lowercase; split on Unicode whitespace; drop empties; join with '-'; keep only chars that are
/// alphabetic, numeric or '-'. Empty → "". If `existing` contains it (case-insensitive), try "{base}-2",
/// "{base}-3", … until unique.
pub fn generate_context_name(name: &str, existing: &[&str]) -> String {
    let joined = name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    let base: String = joined
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_numeric() || *c == '-')
        .collect();
    if base.is_empty() {
        return String::new();
    }
    let taken = |candidate: &str| {
        let want = candidate.to_lowercase();
        existing.iter().any(|e| e.to_lowercase() == want)
    };
    let mut candidate = base.clone();
    let mut counter = 2;
    while taken(&candidate) {
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-OBL-4
    fn r_obl_4_basic() {
        assert_eq!(
            generate_context_name("Property Tax 2026", &[]),
            "property-tax-2026"
        );
    }

    #[test]
    /// Covers: R-OBL-4
    fn r_obl_4_strips_punctuation() {
        assert_eq!(generate_context_name("Kid's  Braces!", &[]), "kids-braces");
    }

    #[test]
    /// Covers: R-OBL-4
    fn r_obl_4_dedupes() {
        assert_eq!(
            generate_context_name("Property Tax", &["property-tax", "Property-Tax-2"]),
            "property-tax-3"
        );
    }

    #[test]
    /// Covers: R-OBL-4
    fn r_obl_4_empty() {
        assert_eq!(generate_context_name("!!!", &[]), "");
    }

    #[test]
    /// Covers: R-OBL-4
    fn r_obl_4_unicode_letters_kept() {
        assert_eq!(generate_context_name("Café Fund", &[]), "café-fund");
    }
}
