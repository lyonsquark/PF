//! Obligation balances: computed balance, reserved and available amounts, grouping by status.

use pf_model::*;
use rust_decimal::Decimal;

use crate::balance::is_visible;

/// R-OBL-1: current_balance + sum of amounts of live transactions carrying the obligation's
/// context and visible under `active` (R-SCN-1). No context gives `current_balance`.
pub fn computed_balance(db: &Db, ob: &Obligation, active: Option<ScenarioId>) -> Decimal {
    let Some(ctx) = ob.context_id else {
        return ob.current_balance;
    };
    let tagged: Decimal = db
        .transactions_with_context(ctx)
        .filter(|tx| is_visible(tx, active))
        .map(|tx| tx.amount)
        .sum();
    ob.current_balance + tagged
}

/// R-OBL-2: sum of computed balances of the account's LIVE obligations with status Funding or Spending.
pub fn total_reserved(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal {
    db.obligations_for_account(account)
        .filter(|ob| {
            matches!(
                ob.status,
                ObligationStatus::Funding | ObligationStatus::Spending
            )
        })
        .map(|ob| computed_balance(db, ob, active))
        .sum()
}

/// R-OBL-3: total balance minus total reserved.
pub fn available_balance(total: Decimal, reserved: Decimal) -> Decimal {
    total - reserved
}

/// R-OBL-5: live obligations grouped by status in ObligationStatus::ALL order, each group sorted by
/// name; empty groups omitted.
pub fn obligations_by_status(db: &Db) -> Vec<(ObligationStatus, Vec<ObligationId>)> {
    let sorted = db.obligations_by_name();
    ObligationStatus::ALL
        .into_iter()
        .filter_map(|status| {
            let ids: Vec<ObligationId> = sorted
                .iter()
                .filter(|ob| ob.status == status)
                .map(|ob| ob.id)
                .collect();
            (!ids.is_empty()).then_some((status, ids))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, m};

    fn ob_with_context(t: &mut TestDb, name: &str) -> (ObligationId, ContextId) {
        let ctx = t.context(name);
        let ob = t.obligation(name).context(ctx).balance("500").done();
        (ob, ctx)
    }

    #[test]
    fn r_obl_1_computed_balance_adds_context_txs() {
        // Covers: R-OBL-1
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let (ob, ctx) = ob_with_context(&mut t, "Car");
        t.tx(chk, "2026-01-05", "200", "Refund")
            .contexts(&[ctx])
            .done();
        t.tx(chk, "2026-01-06", "-50", "Parts")
            .contexts(&[ctx])
            .done();
        t.tx(chk, "2026-01-07", "-9", "Unrelated").done();
        let db = t.build();
        let o = db.obligation(ob).expect("inserted");
        assert_eq!(computed_balance(&db, o, None), m("650"));
    }

    #[test]
    fn r_obl_1_no_context_is_current_balance() {
        // Covers: R-OBL-1
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let ob = t
            .obligation("Savings goal")
            .account(chk)
            .balance("75")
            .done();
        t.tx(chk, "2026-01-06", "-9", "Unrelated").done();
        let db = t.build();
        let o = db.obligation(ob).expect("inserted");
        assert_eq!(computed_balance(&db, o, None), m("75"));
    }

    #[test]
    fn r_obl_1_respects_scenario_visibility() {
        // Covers: R-OBL-1
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        let (ob, ctx) = ob_with_context(&mut t, "Trip");
        t.tx(chk, "2026-04-01", "-100", "Flights")
            .contexts(&[ctx])
            .scenario(s)
            .done();
        t.tx(chk, "2026-04-02", "-20", "Real")
            .contexts(&[ctx])
            .done();
        let db = t.build();
        let o = db.obligation(ob).expect("inserted");
        assert_eq!(computed_balance(&db, o, None), m("480"));
        assert_eq!(computed_balance(&db, o, Some(s)), m("380"));
    }

    #[test]
    fn r_obl_2_reserved_only_funding_spending_live() {
        // Covers: R-OBL-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.obligation("Funding").account(chk).balance("100").done();
        t.obligation("Spending")
            .account(chk)
            .balance("200")
            .status(ObligationStatus::Spending)
            .done();
        t.obligation("Future")
            .account(chk)
            .balance("1000")
            .status(ObligationStatus::Future)
            .done();
        t.obligation("Deferred")
            .account(chk)
            .balance("1000")
            .status(ObligationStatus::Deferred)
            .done();
        t.obligation("Completed")
            .account(chk)
            .balance("1000")
            .status(ObligationStatus::Completed)
            .done();
        let gone = t.obligation("Gone").account(chk).balance("1000").done();
        t.delete(RowKey::Obligation(gone));
        let db = t.build();
        assert_eq!(total_reserved(&db, chk, None), m("300"));
    }

    #[test]
    fn r_obl_3_available_is_total_minus_reserved() {
        // Covers: R-OBL-3
        assert_eq!(available_balance(m("1000"), m("300")), m("700"));
        assert_eq!(available_balance(m("100"), m("250")), m("-150"));
    }

    #[test]
    fn r_obl_5_grouped_by_status_in_order() {
        // Covers: R-OBL-5
        let mut t = TestDb::new();
        let done = t
            .obligation("Done")
            .status(ObligationStatus::Completed)
            .done();
        let zed = t.obligation("Zed").done();
        let amy = t.obligation("Amy").done();
        let later = t
            .obligation("Later")
            .status(ObligationStatus::Future)
            .done();
        let spend = t
            .obligation("Spend")
            .status(ObligationStatus::Spending)
            .done();
        let db = t.build();
        let groups = obligations_by_status(&db);
        assert_eq!(
            groups,
            vec![
                (ObligationStatus::Funding, vec![amy, zed]),
                (ObligationStatus::Spending, vec![spend]),
                (ObligationStatus::Future, vec![later]),
                (ObligationStatus::Completed, vec![done]),
            ]
        );
    }
}
