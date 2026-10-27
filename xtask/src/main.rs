//! Build automation: `cargo xtask rules` (traceability check) and `cargo xtask ci` (full gate).

mod ci;
mod rules;

use anyhow::anyhow;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow!("xtask manifest directory has no parent"))?;

    match std::env::args().nth(1).as_deref() {
        Some("rules") => {
            let cov = rules::check_rules(root)?;
            let covered = cov.declared.saturating_sub(cov.missing.len());
            println!("rules: {covered}/{} IDs covered", cov.declared);
            if !cov.missing.is_empty() {
                println!("missing ({}):", cov.missing.len());
                for id in &cov.missing {
                    println!("  {id}");
                }
                std::process::exit(1);
            }
            Ok(())
        }
        Some("ci") => ci::run_ci(root),
        _ => {
            eprintln!("usage: cargo xtask <rules|ci>");
            std::process::exit(2);
        }
    }
}
