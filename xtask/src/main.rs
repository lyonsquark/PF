//! Build automation: `cargo xtask rules` (traceability check), `cargo xtask ci` (full gate) and
//! `cargo xtask bundle [--install]` (the PF.app bundle, via `scripts/build_app.sh`).

mod ci;
mod rules;

use anyhow::{Context, anyhow};
use std::path::Path;
use std::process::Command;

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
        Some("bundle") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let status = Command::new(root.join("scripts/build_app.sh"))
                .args(&args)
                .current_dir(root)
                .status()
                .context("running scripts/build_app.sh")?;
            std::process::exit(status.code().unwrap_or(1));
        }
        _ => {
            eprintln!("usage: cargo xtask <rules|ci|bundle [--install]>");
            std::process::exit(2);
        }
    }
}
