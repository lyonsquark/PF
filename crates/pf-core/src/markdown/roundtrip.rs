//! R-MD-3: property test that `import(export(db))` matches `db` after mapping ids to names.

use std::collections::HashSet;

use jiff::Timestamp;
use jiff::civil::Date;
use pf_model::{
    Account, AccountId, AccountType, BudgetFrequency, BudgetItem, BudgetItemType, Category,
    CategoryId, CategoryType, Context, ContextId, Db, ImportMapping, Obligation, ObligationId,
    ObligationStatus, Scenario, ScenarioId, Transaction, TransactionId, TransactionStatus,
};
use proptest::prelude::*;
use rust_decimal::Decimal;

use super::helpers::{new_york, now, tokyo};
use super::{export_markdown, import_markdown};

/// Cell values that would collide with the importer's header-row check.
#[cfg(test)]
const HEADER_CELLS: [&str; 4] = ["Name", "Date", "Type", "Key"];

/// Entity names: no leading or trailing space, no `|`, `,`, `;` or `---`.
fn name() -> BoxedStrategy<String> {
    "[A-Za-z][A-Za-z0-9 ]{0,10}[A-Za-z0-9]"
        .prop_filter("header cell or reserved name", |s| {
            !HEADER_CELLS.contains(&s.as_str()) && s != "Uncategorized"
        })
        .boxed()
}

/// Removes every `---` so a free-text cell cannot make a row look like a separator.
fn strip_triple_dash(s: &str) -> String {
    let mut out = s.to_string();
    while out.contains("---") {
        out = out.replace("---", "");
    }
    out
}

/// Free text (payee, memo, notes): trimmed, and possibly containing `|` to exercise escaping.
fn text() -> BoxedStrategy<String> {
    "[A-Za-z0-9 |.,'!-]{0,20}"
        .prop_map(|s| strip_triple_dash(&s).trim().to_string())
        .prop_filter("header cell", |s| !HEADER_CELLS.contains(&s.as_str()))
        .boxed()
}

fn opt_text() -> BoxedStrategy<Option<String>> {
    text().prop_map(|s| (!s.is_empty()).then_some(s)).boxed()
}

fn amount() -> BoxedStrategy<Decimal> {
    (-10_000_000i64..=10_000_000)
        .prop_map(|cents| Decimal::new(cents, 2))
        .boxed()
}

fn date() -> BoxedStrategy<Date> {
    (2020i16..=2030, 1i8..=12, 1i8..=28)
        .prop_map(|(y, m, d)| Date::new(y, m, d).expect("valid date"))
        .boxed()
}

fn timestamp() -> BoxedStrategy<Timestamp> {
    (1_577_836_800i64..=1_893_456_000)
        .prop_map(|s| Timestamp::from_second(s).expect("in range"))
        .boxed()
}

/// Picks one of the rows already built, or none. `None` when the table is empty.
fn pick<T: Copy>(rows: &[T], index: Option<usize>) -> Option<T> {
    if rows.is_empty() {
        return None;
    }
    index.map(|i| rows[i % rows.len()])
}

/// Adds the row only if its name is new for this entity type (R-MD-6 needs unique names).
fn first_time(seen: &mut HashSet<String>, name: &str) -> bool {
    seen.insert(name.to_string())
}

/// A random Db that respects the round-trip guarantee in `docs/markdown-format.md`.
fn db_strategy() -> impl Strategy<Value = Db> {
    let accounts = prop::collection::vec(
        (
            name(),
            prop::sample::select(AccountType::ALL.to_vec()),
            text(),
            prop::sample::select(vec!["USD", "EUR"]),
            any::<bool>(),
        ),
        0..4,
    );
    let contexts = prop::collection::vec((name(), opt_text(), opt_text()), 0..4);
    let scenarios = prop::collection::vec((name(), opt_text(), opt_text()), 0..3);
    let categories = prop::collection::vec(
        (
            name(),
            prop::sample::select(CategoryType::ALL.to_vec()),
            opt_text(),
            opt_text(),
            prop::collection::vec(text().prop_filter("non-empty", |s| !s.is_empty()), 0..3),
            any::<bool>(),
            any::<usize>(),
        ),
        0..6,
    );
    let obligations = prop::collection::vec(
        (
            name(),
            prop::sample::select(ObligationStatus::ALL.to_vec()),
            proptest::option::of(amount()),
            amount(),
            proptest::option::of(date()),
            timestamp(),
            timestamp(),
            opt_text(),
            proptest::option::of(any::<usize>()),
            proptest::option::of(any::<usize>()),
        ),
        0..3,
    );
    let budget_items = prop::collection::vec(
        (
            prop::sample::select(BudgetItemType::ALL.to_vec()),
            name(),
            amount(),
            date(),
            proptest::option::of(date()),
            prop::sample::select(BudgetFrequency::ALL.to_vec()),
            prop::collection::vec(1u32..=12, 0..4),
            opt_text(),
            proptest::option::of(any::<usize>()),
            proptest::option::of(any::<usize>()),
        ),
        0..4,
    );
    let mappings = prop::collection::vec(
        (
            text(),
            text(),
            text(),
            any::<bool>(),
            proptest::option::of(any::<usize>()),
        ),
        0..4,
    );
    let transactions = prop::collection::vec(
        (
            proptest::option::of(any::<usize>()),
            date(),
            amount(),
            text(),
            opt_text(),
            opt_text(),
            prop::sample::select(TransactionStatus::ALL.to_vec()),
            proptest::option::of(any::<usize>()),
            proptest::option::of(any::<usize>()),
            prop::collection::vec(any::<usize>(), 0..3),
        ),
        0..8,
    );
    (
        accounts,
        contexts,
        scenarios,
        categories,
        obligations,
        budget_items,
        mappings,
        transactions,
    )
        .prop_map(build)
}

type Spec = (
    Vec<(String, AccountType, String, &'static str, bool)>,
    Vec<(String, Option<String>, Option<String>)>,
    Vec<(String, Option<String>, Option<String>)>,
    Vec<(
        String,
        CategoryType,
        Option<String>,
        Option<String>,
        Vec<String>,
        bool,
        usize,
    )>,
    Vec<(
        String,
        ObligationStatus,
        Option<Decimal>,
        Decimal,
        Option<Date>,
        Timestamp,
        Timestamp,
        Option<String>,
        Option<usize>,
        Option<usize>,
    )>,
    Vec<(
        BudgetItemType,
        String,
        Decimal,
        Date,
        Option<Date>,
        BudgetFrequency,
        Vec<u32>,
        Option<String>,
        Option<usize>,
        Option<usize>,
    )>,
    Vec<(String, String, String, bool, Option<usize>)>,
    Vec<(
        Option<usize>,
        Date,
        Decimal,
        String,
        Option<String>,
        Option<String>,
        TransactionStatus,
        Option<usize>,
        Option<usize>,
        Vec<usize>,
    )>,
);

/// Builds the Db from a generated spec. Duplicate names are dropped; foreign keys are picks
/// from the rows that were actually created, so every reference is valid.
fn build(spec: Spec) -> Db {
    let (accounts, contexts, scenarios, categories, obligations, budgets, mappings, txs) = spec;
    let mut db = Db::new();
    let mut seen = HashSet::new();

    let mut acc_ids: Vec<AccountId> = Vec::new();
    for (name, ty, institution, currency, external) in accounts {
        if !first_time(&mut seen, &name) {
            continue;
        }
        let id = AccountId::from(db.alloc_id());
        let mut a = Account::new(id, &name, ty, &institution);
        a.currency = currency.to_string();
        a.is_external = external;
        db.put(pf_model::Record::Account(a));
        acc_ids.push(id);
    }

    let mut ctx_ids: Vec<ContextId> = Vec::new();
    seen.clear();
    for (name, color, notes) in contexts {
        if name.contains(',') || !first_time(&mut seen, &name) {
            continue;
        }
        let id = ContextId::from(db.alloc_id());
        let mut c = Context::new(id, &name);
        c.color_hex = color;
        c.notes = notes;
        db.put(pf_model::Record::Context(c));
        ctx_ids.push(id);
    }

    let mut scn_ids: Vec<ScenarioId> = Vec::new();
    seen.clear();
    for (name, color, notes) in scenarios {
        if !first_time(&mut seen, &name) {
            continue;
        }
        let id = ScenarioId::from(db.alloc_id());
        let mut s = Scenario::new(id, &name);
        s.color_hex = color;
        s.notes = notes;
        db.put(pf_model::Record::Scenario(s));
        scn_ids.push(id);
    }

    // Categories: one level only. Children refer to a top-level category built earlier.
    let mut tops: Vec<CategoryId> = Vec::new();
    let mut cat_ids: Vec<CategoryId> = Vec::new();
    seen.clear();
    for (name, ty, icon, notes, patterns, is_child, pick_index) in categories {
        // Spec: patterns may not contain `;` and must be non-empty.
        let patterns: Vec<String> = patterns.into_iter().filter(|p| !p.contains(';')).collect();
        if !first_time(&mut seen, &name) {
            continue;
        }
        let id = CategoryId::from(db.alloc_id());
        let mut c = Category::new(id, &name, ty);
        c.icon = icon;
        c.notes = notes;
        c.match_patterns = patterns;
        if is_child && !tops.is_empty() {
            c.parent_id = Some(tops[pick_index % tops.len()]);
        } else {
            tops.push(id);
        }
        db.put(pf_model::Record::Category(c));
        cat_ids.push(id);
    }

    seen.clear();
    for (name, status, target, balance, due, created, updated, notes, acc, ctx) in obligations {
        if !first_time(&mut seen, &name) {
            continue;
        }
        let mut o = Obligation::new(ObligationId::from(db.alloc_id()), &name);
        o.status = status;
        o.target_amount = target;
        o.current_balance = balance;
        o.due_date = due;
        o.created_date = Some(created);
        o.last_updated_date = Some(updated);
        o.notes = notes;
        o.account_id = pick(&acc_ids, acc);
        o.context_id = pick(&ctx_ids, ctx);
        db.put(pf_model::Record::Obligation(o));
    }

    seen.clear();
    for (ty, name, amount, start, end, freq, months, notes, cat, acc) in budgets {
        if !first_time(&mut seen, &name) {
            continue;
        }
        let mut b = BudgetItem::new(
            pf_model::BudgetItemId::from(db.alloc_id()),
            ty,
            &name,
            amount,
            start,
            freq,
        );
        b.end_date = end;
        b.specific_months = months;
        b.notes = notes;
        b.category_id = pick(&cat_ids, cat);
        b.account_id = pick(&acc_ids, acc);
        db.put(pf_model::Record::BudgetItem(b));
    }

    for (mtype, source, target, negate, acc) in mappings {
        let mut m = ImportMapping::new(
            pf_model::ImportMappingId::from(db.alloc_id()),
            &mtype,
            &source,
            &target,
            pick(&acc_ids, acc),
        );
        m.negate_amounts = negate;
        db.put(pf_model::Record::ImportMapping(m));
    }

    for (acc, date, amount, payee, memo, attachment, status, cat, scn, ctxs) in txs {
        let mut t = Transaction::new(TransactionId::from(db.alloc_id()), date, amount, &payee);
        t.account_id = pick(&acc_ids, acc);
        t.memo = memo;
        t.attachment_path = attachment;
        t.status = status;
        t.category_id = pick(&cat_ids, cat);
        t.scenario_id = pick(&scn_ids, scn);
        t.context_ids = ctxs
            .iter()
            .filter_map(|c| pick(&ctx_ids, Some(*c)))
            .collect();
        db.put(pf_model::Record::Transaction(t));
    }
    db
}

/// Renders every row with ids replaced by names, then sorts. Decimals compare by value.
fn normalise(db: &Db) -> Vec<String> {
    let acct = |id: Option<AccountId>| {
        id.and_then(|i| db.account(i))
            .map_or("-".to_string(), |a| a.name.clone())
    };
    let scn = |id: Option<pf_model::ScenarioId>| {
        id.and_then(|i| db.scenario(i))
            .map_or("-".to_string(), |s| s.name.clone())
    };
    let cat = |id: Option<CategoryId>| {
        id.and_then(|i| db.category(i))
            .map_or("-".to_string(), |c| c.name.clone())
    };
    let mut out = Vec::new();
    for a in db.accounts.values() {
        out.push(format!(
            "account|{}|{}|{}|{}|{}",
            a.name,
            a.account_type.as_str(),
            a.institution,
            a.currency,
            a.is_external
        ));
    }
    for c in db.contexts.values() {
        out.push(format!(
            "context|{}|{:?}|{:?}",
            c.name, c.color_hex, c.notes
        ));
    }
    for s in db.scenarios.values() {
        out.push(format!(
            "scenario|{}|{:?}|{:?}",
            s.name, s.color_hex, s.notes
        ));
    }
    for c in db.categories.values() {
        let parent = c
            .parent_id
            .and_then(|p| db.category(p))
            .map(|p| p.name.clone());
        out.push(format!(
            "category|{}|{}|{:?}|{:?}|{}|{:?}",
            c.name,
            c.category_type.as_str(),
            c.icon,
            c.notes,
            c.match_patterns.join(";"),
            parent
        ));
    }
    for o in db.obligations.values() {
        out.push(format!(
            "obligation|{}|{}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            o.name,
            o.status.as_str(),
            o.target_amount.map(|d| d.normalize()),
            o.current_balance.normalize(),
            o.due_date,
            o.created_date,
            o.last_updated_date,
            o.notes,
            o.account_id
                .and_then(|a| db.account(a))
                .map(|a| a.name.clone()),
            o.context_id
                .and_then(|c| db.context(c))
                .map(|c| c.name.clone()),
        ));
    }
    for b in db.budget_items.values() {
        out.push(format!(
            "budget|{}|{}|{}|{}|{:?}|{}|{:?}|{:?}|{}|{}",
            b.name,
            b.item_type.as_str(),
            b.amount.normalize(),
            b.start_date,
            b.end_date,
            b.frequency.as_str(),
            format!("{:?}", b.specific_months),
            b.notes,
            cat(b.category_id),
            acct(b.account_id),
        ));
    }
    for m in db.import_mappings.values() {
        out.push(format!(
            "mapping|{}|{}|{}|{}|{}",
            m.mapping_type,
            m.source_value,
            m.target_value,
            m.negate_amounts,
            acct(m.account_id)
        ));
    }
    for t in db.transactions.values() {
        let mut ctx_names: Vec<String> = t
            .context_ids
            .iter()
            .filter_map(|c| db.context(*c).map(|c| c.name.clone()))
            .collect();
        ctx_names.sort();
        out.push(format!(
            "tx|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}|{}",
            t.date,
            t.amount.normalize(),
            t.payee,
            t.memo,
            t.attachment_path,
            t.status.as_str(),
            acct(t.account_id),
            scn(t.scenario_id),
            cat(t.category_id),
            ctx_names.join(",")
        ));
    }
    out.sort();
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Covers: R-MD-3
    #[test]
    fn r_md_3_round_trip_property(db in db_strategy()) {
        for tz in [new_york(), tokyo()] {
            let text = export_markdown(&db, &tz);
            let back = import_markdown(&text, &tz, now(), &mut |_| {}).db;
            prop_assert_eq!(normalise(&back), normalise(&db), "exported:\n{}", text);
        }
    }
}
