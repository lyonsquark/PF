//! R-REC-11: normalising and scoring.

use crate::reconcile::{normalize_description as n, payee_score as score};

fn aliases(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// Covers: R-REC-11
#[test]
fn r_rec_11_normalize_and_score() {
    assert_eq!(n("SQ *BLUE BOTTLE 8812"), "BLUE BOTTLE");
    assert_eq!(n("TST* JOE'S PIZZA #12"), "JOE S PIZZA");
    assert_eq!(n("AMAZON.COM*2K4 AMZN.COM/BILL WA"), "AMAZON AMZN BILL WA");
    assert_eq!(n("CHECKCARD 0914 TRADER JOE'S #552"), "TRADER JOE S");
    assert_eq!(n(""), "");
    assert_eq!(
        score("SQ *BLUE BOTTLE 8812", "Blue Bottle Coffee", &[]),
        1.0
    );
    assert_eq!(
        score("CHECKCARD 0914 TRADER JOE'S #552", "Trader Joe's", &[]),
        1.0
    );
    assert_eq!(score("AMAZON.COM*2K4 AMZN.COM/BILL WA", "Amazon", &[]), 1.0);
    assert_eq!(score("SHELL OIL 5512", "Chevron", &[]), 0.0);
    assert_eq!(score("", "Chevron", &[]), 0.0);
    assert_eq!(score("SHELL OIL", "", &[]), 0.0);
    assert_eq!(score("ACME PAYROLL DEPOSIT", "Acme Corp", &[]), 0.5);
    let a = aliases(&[("AMZN MKTP US", "Amazon")]);
    assert_eq!(score("AMZN Mktp US*1A2", "amazon", &a), 1.0);
    assert_eq!(score("AMZN Mktp US*1A2", "Target", &a), 0.0);
}
