//! The full local gate: fmt check, clippy, tests, then the traceability check.

use crate::rules::run_rules;
use anyhow::{Context, bail};
use std::path::Path;
use std::process::Command;

/// Runs fmt check, clippy, tests and the rules check, stopping at the first failure.
pub fn run_ci(root: &Path) -> anyhow::Result<()> {
    let steps: [&[&str]; 3] = [
        &["fmt", "--all", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        &["test", "--workspace"],
    ];
    for args in steps {
        let line = args.join(" ");
        println!("+ cargo {line}");
        let status = Command::new("cargo")
            .args(args)
            .current_dir(root)
            .status()
            .with_context(|| format!("running cargo {line}"))?;
        if !status.success() {
            bail!("cargo {line} failed ({status})");
        }
    }

    println!("+ rules");
    let missing = run_rules(root)?;
    if !missing.is_empty() {
        println!("missing ({}):", missing.len());
        for id in &missing {
            println!("  {id}");
        }
        bail!("rules: {} IDs have no test", missing.len());
    }
    println!("rules: all IDs covered");
    Ok(())
}
