//! Memoised derived data. `Cached` recomputes only when the engine revision or the key changes,
//! so lists are never rebuilt every frame (PLAN.md, UI rules).

use pf_core::categorize::{categories_for_picker, is_emoji};
use pf_engine::Engine;
use pf_model::{AccountId, AccountType, CategoryId, ContextId, Db, ScenarioId};

/// One value keyed by `(engine revision, key)`.
pub struct Cached<K, V> {
    entry: Option<(u64, K, V)>,
}

impl<K, V> Default for Cached<K, V> {
    fn default() -> Self {
        Self { entry: None }
    }
}

impl<K: PartialEq + Clone, V> Cached<K, V> {
    /// Returns the stored value when `revision` and `key` match; otherwise calls `compute`.
    pub fn get(&mut self, revision: u64, key: &K, compute: impl FnOnce() -> V) -> &V {
        let fresh = matches!(&self.entry, Some((r, k, _)) if *r == revision && k == key);
        if !fresh {
            self.entry = None;
        }
        &self
            .entry
            .get_or_insert_with(|| (revision, key.clone(), compute()))
            .2
    }

    /// Drops the stored value; the next `get` recomputes.
    pub fn invalidate(&mut self) {
        self.entry = None;
    }
}

/// One account as the sidebar needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    /// Account id.
    pub id: AccountId,
    /// Display name.
    pub name: String,
    /// Institution as typed (may be empty).
    pub institution: String,
    /// Kind of account.
    pub account_type: AccountType,
    /// External accounts get the globe prefix.
    pub is_external: bool,
}

/// Lists used by pickers and the sidebar across many screens.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lookups {
    /// Live accounts, sorted by name (case-insensitive).
    pub accounts: Vec<(AccountId, String)>,
    /// Live accounts with their sidebar fields, sorted by name (case-insensitive).
    pub account_rows: Vec<AccountRow>,
    /// Category picker list (R-CAT-4): sorted by full name, emoji icons prefixed.
    pub categories: Vec<(CategoryId, String)>,
    /// Live, not archived contexts sorted by name: (id, name, colour hex).
    pub contexts: Vec<(ContextId, String, Option<String>)>,
    /// Live scenarios sorted by name.
    pub scenarios: Vec<(ScenarioId, String)>,
}

/// Builds every lookup list from `db`. Deleted rows are never included.
pub fn build_lookups(db: &Db) -> Lookups {
    let mut account_rows: Vec<AccountRow> = db
        .live_accounts()
        .map(|a| AccountRow {
            id: a.id,
            name: a.name.clone(),
            institution: a.institution.clone(),
            account_type: a.account_type,
            is_external: a.is_external,
        })
        .collect();
    account_rows.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    let accounts = account_rows
        .iter()
        .map(|r| (r.id, r.name.clone()))
        .collect();

    let categories = categories_for_picker(db)
        .into_iter()
        .map(|(id, name)| {
            let icon = db.category(id).and_then(|c| c.icon.as_deref());
            match icon {
                Some(icon) if is_emoji(icon) => (id, format!("{icon} {name}")),
                _ => (id, name),
            }
        })
        .collect();

    let mut contexts: Vec<(ContextId, String, Option<String>)> = db
        .live_contexts()
        .filter(|c| !c.is_archived)
        .map(|c| (c.id, c.name.clone(), c.color_hex.clone()))
        .collect();
    contexts.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });

    let mut scenarios: Vec<(ScenarioId, String)> = db
        .live_scenarios()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    scenarios.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });

    Lookups {
        accounts,
        account_rows,
        categories,
        contexts,
        scenarios,
    }
}

/// View-level caches shared by every screen.
#[derive(Default)]
pub struct ViewCaches {
    lookups: Cached<(), Lookups>,
}

impl ViewCaches {
    /// The shared lookups, rebuilt only when the engine revision changes.
    pub fn lookups(&mut self, engine: &Engine) -> &Lookups {
        self.lookups
            .get(engine.revision(), &(), || build_lookups(engine.db()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;
    use pf_model::{CategoryType, RowKey};

    #[test]
    /// Covers: UI caches (T13 spec). Same (rev, key) computes once; new rev or new key recomputes.
    fn cached_recomputes_only_on_revision_or_key_change() {
        let mut calls = 0;
        let mut cache: Cached<&'static str, u32> = Cached::default();
        let mut get = |cache: &mut Cached<&'static str, u32>, rev: u64, key: &'static str| {
            *cache.get(rev, &key, || {
                calls += 1;
                7
            })
        };
        assert_eq!(get(&mut cache, 1, "a"), 7);
        assert_eq!(get(&mut cache, 1, "a"), 7);
        assert_eq!(get(&mut cache, 2, "a"), 7);
        assert_eq!(get(&mut cache, 2, "b"), 7);
        drop(get);
        assert_eq!(calls, 3);
    }

    #[test]
    /// Covers: UI caches (T13 spec). Invalidate forces a recompute.
    fn cached_invalidate_forces_recompute() {
        let mut cache: Cached<(), u32> = Cached::default();
        let mut n = 0;
        cache.get(1, &(), || {
            n += 1;
            n
        });
        cache.invalidate();
        let v = *cache.get(1, &(), || {
            n += 1;
            n
        });
        assert_eq!(v, 2);
    }

    #[test]
    /// Covers: R-CAT-4 (picker sorted by full name), R-ACC-4 (live accounts only), R-CTX-4.
    fn lookups_sorted_and_live_only() {
        let mut t = TestDb::new();
        let beta = t.account("beta", AccountType::Checking);
        t.account("Alpha", AccountType::Savings);
        let gone = t.account("Gone", AccountType::Cash);
        t.delete(RowKey::Account(gone));
        let parent = t.category("zeta", CategoryType::Expense);
        t.child_category(parent, "kid");
        t.category("Apple", CategoryType::Income);
        t.context("Vacation");
        t.context("archived");
        t.scenario("Plan B");
        let deleted_scn = t.scenario("Old");
        t.delete(RowKey::Scenario(deleted_scn));
        let mut db = t.build();
        let archived = db.context_by_name("archived").map(|c| c.id);
        if let Some(c) = archived.and_then(|id| db.contexts.get_mut(&id)) {
            c.is_archived = true;
        }

        let l = build_lookups(&db);
        let names: Vec<&str> = l.accounts.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "beta"]);
        assert!(l.accounts.iter().all(|(id, _)| *id != gone));
        assert_eq!(l.accounts[1].0, beta);

        let cats: Vec<&str> = l.categories.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(cats, vec!["Apple", "zeta", "zeta/kid"]);

        let ctxs: Vec<&str> = l.contexts.iter().map(|(_, n, _)| n.as_str()).collect();
        assert_eq!(ctxs, vec!["Vacation"]);

        let scns: Vec<&str> = l.scenarios.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(scns, vec!["Plan B"]);
    }
}
