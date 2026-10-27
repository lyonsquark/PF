//! Traceability check: every rule and behaviour ID declared in the spec docs
//! must be referenced by at least one test in `crates/` or `apps/`.

use anyhow::{Context, anyhow};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Matches declared IDs such as `R-PRJ-3`, `R-RPT-4a`, `BEH-ACC-01`, `UI-NAV-01`.
const ID_PATTERN: &str = r"\b(R-[A-Z]+-\d+[a-z]?|BEH-[A-Z]+-\d+|UI-[A-Z]+-\d+)\b";
/// Matches test names such as `r_prj_3_jan31` (the trailing separator is consumed).
const NAME_PATTERN: &str =
    r"(?i)(?:^|[^A-Za-z0-9])(r|beh|ui)_([a-z]+)_(\d+)([a-z])?(?:_|[^A-Za-z0-9]|$)";

/// Spec docs that declare IDs.
const DECLARING_DOCS: [&str; 2] = ["docs/business-rules.md", "docs/testing.md"];
/// Directories (relative to the workspace root) scanned for test references.
const SOURCE_ROOTS: [&str; 2] = ["crates", "apps"];

static ID_RE: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| Regex::new(ID_PATTERN));
static NAME_RE: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| Regex::new(NAME_PATTERN));

/// Result of the traceability check.
pub struct Coverage {
    /// Number of distinct IDs declared in the spec docs.
    pub declared: usize,
    /// Declared IDs with no test reference, sorted.
    pub missing: Vec<String>,
}

/// IDs declared in the spec docs, e.g. "R-PRJ-3", "R-RPT-4a", "BEH-ACC-01", "UI-NAV-01".
pub fn declared_ids(doc_text: &str) -> BTreeSet<String> {
    let Some(re) = id_re() else {
        return BTreeSet::new();
    };
    re.captures_iter(doc_text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// IDs referenced from test code in one Rust source file.
///
/// Files under a `tests` directory are scanned whole. Other files are scanned only from
/// the first `#[cfg(test)]` onwards, so implementation comments do not count as tests.
pub fn referenced_ids(path: &Path, source: &str) -> BTreeSet<String> {
    let in_tests_dir = path.components().any(|c| c.as_os_str() == "tests");
    let scope = if in_tests_dir {
        source
    } else {
        match source.find("#[cfg(test)]") {
            Some(i) => source.get(i..).unwrap_or_default(),
            None => return BTreeSet::new(),
        }
    };

    let mut out = declared_ids(scope);
    if let Some(re) = name_re() {
        for c in re.captures_iter(scope) {
            let group = |n: usize| c.get(n).map_or("", |m| m.as_str());
            let prefix = group(1).to_uppercase();
            let family = group(2).to_uppercase();
            let number = group(3);
            let suffix = group(4).to_lowercase();
            out.insert(format!("{prefix}-{family}-{number}{suffix}"));
        }
    }
    out
}

/// Runs the whole check from the workspace root and returns the missing IDs, sorted.
pub fn run_rules(root: &Path) -> anyhow::Result<Vec<String>> {
    Ok(check_rules(root)?.missing)
}

/// Runs the whole check from the workspace root and returns declared count and missing IDs.
pub fn check_rules(root: &Path) -> anyhow::Result<Coverage> {
    static_regex(&ID_RE)?;
    static_regex(&NAME_RE)?;

    let mut declared = BTreeSet::new();
    for doc in DECLARING_DOCS {
        let path = root.join(doc);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        declared.extend(declared_ids(&text));
    }

    let mut referenced = BTreeSet::new();
    for dir in SOURCE_ROOTS {
        let mut files = Vec::new();
        rust_files(&root.join(dir), &mut files)?;
        for file in files {
            let source = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let rel = file.strip_prefix(root).unwrap_or(file.as_path());
            referenced.extend(referenced_ids(rel, &source));
        }
    }

    let missing = declared.difference(&referenced).cloned().collect();
    Ok(Coverage {
        declared: declared.len(),
        missing,
    })
}

/// Collects `*.rs` files under `dir` recursively, skipping any `target` directory.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Returns the compiled static regex, or an error if its pattern is invalid.
fn static_regex(
    re: &'static LazyLock<Result<Regex, regex::Error>>,
) -> anyhow::Result<&'static Regex> {
    re.as_ref()
        .map_err(|e| anyhow!("invalid static regex in xtask: {e}"))
}

/// The ID regex, or `None` if it failed to compile (`check_rules` reports that case).
fn id_re() -> Option<&'static Regex> {
    static_regex(&ID_RE).ok()
}

/// The test-name regex, or `None` if it failed to compile (`check_rules` reports that case).
fn name_re() -> Option<&'static Regex> {
    static_regex(&NAME_RE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn patterns_compile() {
        assert!(static_regex(&ID_RE).is_ok());
        assert!(static_regex(&NAME_RE).is_ok());
    }

    #[test]
    fn declared_finds_all_three_kinds() {
        assert_eq!(
            declared_ids("R-PRJ-3 and BEH-ACC-01, UI-NAV-01"),
            ids(&["R-PRJ-3", "BEH-ACC-01", "UI-NAV-01"])
        );
    }

    #[test]
    fn declared_keeps_letter_suffix() {
        assert_eq!(declared_ids("R-RPT-4a…"), ids(&["R-RPT-4a"]));
    }

    #[test]
    fn declared_ignores_regex_literal() {
        assert!(declared_ids(r"R-[A-Z]+-\d+[a-z]?").is_empty());
    }

    #[test]
    fn referenced_test_name_spelling() {
        let src = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn r_prj_3_jan31_clamps() {}\n}\n";
        assert_eq!(
            referenced_ids(Path::new("crates/pf-core/src/lib.rs"), src),
            ids(&["R-PRJ-3"])
        );
    }

    #[test]
    fn referenced_beh_keeps_leading_zero() {
        let src = "#[cfg(test)]\nfn beh_acc_01_create() {}\n";
        assert_eq!(
            referenced_ids(Path::new("crates/pf-engine/src/x.rs"), src),
            ids(&["BEH-ACC-01"])
        );
    }

    #[test]
    fn referenced_suffix_letter() {
        let src = "#[cfg(test)]\nfn r_rpt_4a_balance_sheet() {}\n";
        assert_eq!(
            referenced_ids(Path::new("crates/pf-core/src/x.rs"), src),
            ids(&["R-RPT-4a"])
        );
    }

    #[test]
    fn referenced_covers_comment() {
        let src = "#[cfg(test)]\n/// Covers: R-BAL-1, R-SCN-1\nfn check() {}\n";
        assert_eq!(
            referenced_ids(Path::new("crates/pf-core/src/x.rs"), src),
            ids(&["R-BAL-1", "R-SCN-1"])
        );
    }

    #[test]
    fn impl_comment_outside_cfg_test_ignored() {
        let src = "// R-PRJ-3\nfn f() {}\n#[cfg(test)]\nmod tests {}\n";
        assert!(referenced_ids(Path::new("crates/pf-core/src/x.rs"), src).is_empty());
    }

    #[test]
    fn file_without_cfg_test_counts_nothing() {
        let src = "fn beh_acc_01() {}\n// R-BAL-1\n";
        assert!(referenced_ids(Path::new("crates/pf-core/src/lib.rs"), src).is_empty());
    }

    #[test]
    fn tests_dir_scans_whole_file() {
        let src = "fn beh_scn_02() {}\n";
        assert_eq!(
            referenced_ids(Path::new("crates/x/tests/a.rs"), src),
            ids(&["BEH-SCN-02"])
        );
    }

    #[test]
    fn no_false_positive_on_words() {
        let src = "#[cfg(test)]\nmod t {\n    fn error_handling() {}\n    fn x() { let ui_state = 1; }\n}\n";
        assert!(referenced_ids(Path::new("crates/pf-core/src/x.rs"), src).is_empty());
    }
}
