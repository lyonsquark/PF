//! R-CHT-9: the money-flow Sankey (income → expense groups → subcategories, plus savings) and
//! its node drill-down (R-CHT-9c).

use std::collections::BTreeMap;

use jiff::civil::Date;
use pf_model::{Db, Transaction, TransactionId};
use rust_decimal::Decimal;

use super::{
    COLOR_INCOME, COLOR_SAVINGS, ChartFilter, category_hex, category_name_or_uncategorized,
    group_name, live_subcategory, month_label_long, month_txs, sorted_by_amount,
};
use crate::reports::live_category;

/// R-CHT-9b: links of one cent or less are dropped.
const MIN_LINK: Decimal = Decimal::from_parts(1, 0, 0, false, 2);

/// A node of the Sankey.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyNode {
    /// Unique node name.
    pub name: String,
    /// `#rrggbb` colour.
    pub color: String,
}

/// A flow from one node to another; `source` and `target` index `Sankey::nodes`.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyLink {
    /// Index of the source node.
    pub source: usize,
    /// Index of the target node.
    pub target: usize,
    /// Amount of the flow.
    pub value: Decimal,
}

/// The Sankey for one month. Empty `nodes` means "Not enough data".
#[derive(Debug, Clone, PartialEq)]
pub struct Sankey {
    /// "MMMM yyyy" of the base date.
    pub month_label: String,
    /// Σ of positive amounts.
    pub total_in: Decimal,
    /// Σ of |non-positive amounts|.
    pub total_out: Decimal,
    /// Nodes in display order.
    pub nodes: Vec<SankeyNode>,
    /// Links in node order.
    pub links: Vec<SankeyLink>,
}

/// R-CHT-9, R-CHT-9a, R-CHT-9b: the money flow for the month of `base_date` (F⁻ᵗ).
pub fn sankey(db: &Db, f: &ChartFilter, base_date: Date) -> Sankey {
    let rows = month_txs(db, f, base_date);
    let mut income: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut expense: BTreeMap<String, Decimal> = BTreeMap::new();
    // group → subcategory → amount, only for subcategories of a live parent (R-CHT-9)
    let mut subs: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
    for tx in &rows {
        if tx.amount > Decimal::ZERO {
            // R-CHT-9: income by category name, "Income" when there is no category
            let name = live_category(db, tx.category_id)
                .map_or_else(|| "Income".to_string(), |c| c.name.clone());
            *income.entry(name).or_default() += tx.amount;
        } else {
            let group = group_name(db, tx);
            *expense.entry(group.clone()).or_default() += tx.amount.abs();
            if let Some(sub) = live_subcategory(db, tx) {
                *subs
                    .entry(group)
                    .or_default()
                    .entry(sub.name.clone())
                    .or_default() += tx.amount.abs();
            }
        }
    }

    let total_in: Decimal = income.values().sum();
    let total_out: Decimal = expense.values().sum();
    let mut out = Sankey {
        month_label: month_label_long(base_date),
        total_in,
        total_out,
        nodes: Vec::new(),
        links: Vec::new(),
    };
    // R-CHT-9: empty when either side is missing (fix)
    if income.is_empty() || expense.is_empty() {
        return out;
    }

    // R-CHT-9: savings = max(0, income − expenses)
    let savings = (total_in - total_out).max(Decimal::ZERO);
    let income_sorted = sorted_by_amount(income);
    let expense_sorted = sorted_by_amount(expense);

    // R-CHT-9a: node order, unique by name (first occurrence wins)
    let mut nodes: Vec<SankeyNode> = Vec::new();
    for (name, _) in &income_sorted {
        push_node(&mut nodes, name, COLOR_INCOME.to_string());
    }
    for (name, _) in &expense_sorted {
        push_node(&mut nodes, name, category_hex(name));
    }
    for sub_map in subs.values() {
        for (sub, _) in sorted_by_amount(sub_map.clone()) {
            push_node(&mut nodes, &sub, category_hex(&sub));
        }
    }
    if savings > Decimal::ZERO {
        push_node(&mut nodes, "Savings", COLOR_SAVINGS.to_string());
    }

    // R-CHT-9b: income → each expense group, then income → savings, per income source
    let mut links: Vec<SankeyLink> = Vec::new();
    for (inc_name, inc_amt) in &income_sorted {
        for (grp_name, grp_amt) in &expense_sorted {
            let value = inc_amt * grp_amt / total_in;
            push_link(&mut links, &nodes, inc_name, grp_name, value);
        }
        if savings > Decimal::ZERO {
            let value = inc_amt * savings / total_in;
            push_link(&mut links, &nodes, inc_name, "Savings", value);
        }
    }
    // R-CHT-9b: expense group → subcategory
    for (group, sub_map) in &subs {
        for (sub, amt) in sorted_by_amount(sub_map.clone()) {
            push_link(&mut links, &nodes, group, &sub, amt);
        }
    }

    out.nodes = nodes;
    out.links = links;
    out
}

/// Appends a node unless one with the same name exists.
fn push_node(nodes: &mut Vec<SankeyNode>, name: &str, color: String) {
    if !nodes.iter().any(|n| n.name == name) {
        nodes.push(SankeyNode {
            name: name.to_string(),
            color,
        });
    }
}

/// Appends a link when its value is above one cent and both names are nodes.
fn push_link(
    links: &mut Vec<SankeyLink>,
    nodes: &[SankeyNode],
    source: &str,
    target: &str,
    value: Decimal,
) {
    if value <= MIN_LINK {
        return;
    }
    let index = |name: &str| nodes.iter().position(|n| n.name == name);
    if let (Some(source), Some(target)) = (index(source), index(target)) {
        links.push(SankeyLink {
            source,
            target,
            value,
        });
    }
}

/// R-CHT-9c: the node drill-down: (title, transaction ids in (date, id) order). "Savings" ⇒
/// all F⁻ᵗ rows of the month; any other name (trimmed) ⇒ rows whose group name, category name,
/// or (category name, else "Income" for positive / "Uncategorized" otherwise) equals it.
pub fn sankey_drill(
    db: &Db,
    f: &ChartFilter,
    base_date: Date,
    node: &str,
) -> (String, Vec<TransactionId>) {
    let name = node.trim();
    let rows = month_txs(db, f, base_date);
    let ids: Vec<TransactionId> = if name == "Savings" {
        rows.iter().map(|tx| tx.id).collect()
    } else {
        rows.iter()
            .copied()
            .filter(|tx| matches_node(db, tx, name))
            .map(|tx| tx.id)
            .collect()
    };
    (format!("{name} — {}", month_label_long(base_date)), ids)
}

/// R-CHT-9c: does `tx` belong to the node named `name`?
fn matches_node(db: &Db, tx: &Transaction, name: &str) -> bool {
    let cat_name = live_category(db, tx.category_id).map(|c| c.name.as_str());
    let fallback = if tx.amount > Decimal::ZERO {
        "Income"
    } else {
        "Uncategorized"
    };
    group_name(db, tx) == name
        || category_name_or_uncategorized(db, tx) == name
        || cat_name.unwrap_or(fallback) == name
}

#[cfg(test)]
mod tests {
    use pf_model::testing::{TestDb, d, m};
    use pf_model::{AccountType, CategoryType};

    use super::*;
    use crate::charts::fixtures::{account_id, small_db, small_filter};

    /// (name, colour) of each node.
    fn nodes(s: &Sankey) -> Vec<(&str, &str)> {
        s.nodes
            .iter()
            .map(|n| (n.name.as_str(), n.color.as_str()))
            .collect()
    }

    #[test]
    /// Covers: R-CHT-9
    fn r_cht_9_sankey_empty_rules() {
        let db = small_db();
        let f = small_filter(&db);
        // March has no Reality income (the transfer in is excluded).
        let mar = sankey(&db, &f, d("2026-03-15"));
        assert!(mar.nodes.is_empty());
        assert_eq!(mar.total_in, m("0"));

        let empty = Db::default();
        let f2 = ChartFilter {
            accounts: Default::default(),
            scenario: None,
            today: d("2026-03-15"),
        };
        assert!(sankey(&empty, &f2, d("2026-03-15")).nodes.is_empty());
    }

    #[test]
    /// Covers: R-CHT-9a
    fn r_cht_9a_sankey_nodes_feb() {
        let db = small_db();
        let f = small_filter(&db);
        let s = sankey(&db, &f, d("2026-02-15"));
        assert_eq!(s.month_label, "February 2026");
        assert_eq!(
            nodes(&s),
            vec![
                ("Income", "#34c759"),
                ("Housing", category_hex("Housing").as_str()),
                ("Dining", category_hex("Dining").as_str()),
                ("Utilities", category_hex("Utilities").as_str()),
                ("Savings", "#30d158"),
            ]
        );
    }

    #[test]
    /// Covers: R-CHT-9b
    fn r_cht_9b_sankey_links_feb() {
        let db = small_db();
        let f = small_filter(&db);
        let s = sankey(&db, &f, d("2026-02-15"));
        let links: Vec<(&str, &str, Decimal)> = s
            .links
            .iter()
            .map(|l| {
                (
                    s.nodes[l.source].name.as_str(),
                    s.nodes[l.target].name.as_str(),
                    l.value,
                )
            })
            .collect();
        assert_eq!(
            links,
            vec![
                ("Income", "Housing", m("1653.09")),
                ("Income", "Dining", m("88")),
                ("Income", "Savings", m("2258.91")),
                ("Housing", "Utilities", m("88.10")),
            ]
        );
    }

    #[test]
    /// Covers: R-CHT-9b
    fn r_cht_9b_sankey_split_and_tiny_links() {
        // Two income sources, one expense group of 400, and a group of one tenth of a cent.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let salary = t.category("Salary", CategoryType::Income);
        let bonus = t.category("Bonus", CategoryType::Income);
        let food = t.category("Food", CategoryType::Expense);
        let tiny = t.category("Tiny", CategoryType::Expense);
        t.tx(chk, "2026-02-01", "3000", "Pay")
            .category(salary)
            .done();
        t.tx(chk, "2026-02-02", "1000", "Bonus")
            .category(bonus)
            .done();
        t.tx(chk, "2026-02-03", "-400", "Market")
            .category(food)
            .done();
        t.tx(chk, "2026-02-04", "-0.001", "Dust")
            .category(tiny)
            .done();
        let db = t.build();
        let f = ChartFilter {
            accounts: [account_id(&db, "Checking")].into_iter().collect(),
            scenario: None,
            today: d("2026-03-15"),
        };
        let s = sankey(&db, &f, d("2026-02-15"));
        let names: Vec<&str> = s.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Salary", "Bonus", "Food", "Tiny", "Savings"]);
        // Each income splits in proportion to its share of the 4000 income.
        let link = |src: &str, dst: &str| {
            s.links
                .iter()
                .find(|l| s.nodes[l.source].name == src && s.nodes[l.target].name == dst)
                .map(|l| l.value.round_dp(2))
        };
        assert_eq!(link("Salary", "Food"), Some(m("300")));
        assert_eq!(link("Bonus", "Food"), Some(m("100")));
        assert_eq!(link("Salary", "Savings"), Some(m("2700.00")));
        assert_eq!(link("Bonus", "Savings"), Some(m("900.00")));
        // A link of one cent or less is dropped.
        assert_eq!(link("Salary", "Tiny"), None);
        assert_eq!(link("Bonus", "Tiny"), None);
    }

    #[test]
    /// Covers: R-CHT-9c
    fn r_cht_9c_sankey_drill() {
        let db = small_db();
        let f = small_filter(&db);
        let base = d("2026-02-15");
        let payees = |node: &str| -> Vec<String> {
            sankey_drill(&db, &f, base, node)
                .1
                .iter()
                .filter_map(|id| db.transaction(*id))
                .map(|tx| tx.payee.clone())
                .collect()
        };
        assert_eq!(payees("Utilities"), ["City Water"]);
        assert_eq!(payees("Income"), ["Employer Payroll"]);
        assert_eq!(payees("Savings").len(), 5);
        assert_eq!(payees(" Housing ").len(), 3);
        let (title, _) = sankey_drill(&db, &f, base, "Utilities");
        assert_eq!(title, "Utilities — February 2026");
        let (title, _) = sankey_drill(&db, &f, base, " Housing ");
        assert_eq!(title, "Housing — February 2026");
    }
}
