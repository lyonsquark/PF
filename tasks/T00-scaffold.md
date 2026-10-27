# T00: Workspace scaffold

**Phase:** 0 · **Deps:** none · **Size:** small (copy exact files)

## Goal

Create the Cargo workspace in `PFRust/` exactly as below. These files were **built, tested,
clippy-checked and rustfmt-checked** on 2026-10-26 with rustc 1.99. Copy them verbatim:
do not "improve" versions or features. Every later task depends on this layout.

## Read first

- `PLAN.md` (Decisions, Architecture)
- `CLAUDE.md`
- `docs/egui-cheatsheet.md` (skim)

## Steps

1. Create the directories: `crates/{pf-model,pf-core,pf-store,pf-engine,pf-api,pf-ui}/src`,
   `apps/{pf,pf-cli}/src`, `xtask/src`, `.cargo`, `fixtures`, `scripts`.
2. Create each file below with exactly the content shown.
3. Run the "Done when" commands. The first build downloads and compiles about 400 crates (a few minutes).
4. Run `cargo run -p pf` once and confirm that a window titled "PF" shows "PFRust / Scaffold OK". Close it.

## Files

### `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["crates/*", "apps/*", "xtask"]
default-members = ["crates/*", "apps/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
publish = false

[workspace.dependencies]
# internal
pf-model = { path = "crates/pf-model" }
pf-core = { path = "crates/pf-core" }
pf-store = { path = "crates/pf-store" }
pf-engine = { path = "crates/pf-engine" }
pf-api = { path = "crates/pf-api" }
pf-ui = { path = "crates/pf-ui" }

# foundation
rust_decimal = { version = "1.43", features = ["serde-with-str"] }
rust_decimal_macros = "1.40"
jiff = { version = "0.2", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
log = "0.4"
env_logger = "0.11"
regex = "1.13"

# storage / io
rusqlite = { version = "0.41", features = ["bundled"] }
tiny_http = "0.12"
directories = "6"

# ui
eframe = { version = "0.37", default-features = false, features = ["default_fonts", "wgpu", "accesskit"] }
egui = "0.37"
egui_extras = { version = "0.37", default-features = false, features = ["datepicker"] }
egui_plot = "0.38"
rfd = "0.17"

# tests
proptest = "1.12"
insta = { version = "1.49", features = ["json"] }
pretty_assertions = "1.4"
tempfile = "3.27"
assert_cmd = "2.2"
predicates = "3.1"
egui_kittest = "0.37"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
dbg_macro = "warn"
todo = "warn"
unwrap_used = "warn"
too_many_arguments = "allow"
type_complexity = "allow"

[profile.dev.package."*"]
opt-level = 2

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 16
```

### `.cargo/config.toml`

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

### `rust-toolchain.toml`

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

### `.gitignore`

```
/target
*.pending-snap
*.snap.new
```

### `crates/pf-model/Cargo.toml`

```toml
[package]
name = "pf-model"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
rust_decimal = { workspace = true }
jiff = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
rust_decimal_macros = { workspace = true }
pretty_assertions = { workspace = true }
```

### `crates/pf-model/src/lib.rs`

```rust
//! Entities, enums, the in-memory `Db`, and the `TestDb` builder. No IO.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}
```

### `crates/pf-core/Cargo.toml`

```toml
[package]
name = "pf-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
rust_decimal = { workspace = true }
jiff = { workspace = true }
regex = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
rust_decimal_macros = { workspace = true }
proptest = { workspace = true }
insta = { workspace = true }
pretty_assertions = { workspace = true }
```

### `crates/pf-core/src/lib.rs`

```rust
//! Pure business logic: balances, filters, projector, categorize, markdown, CSV, reports, charts. No IO, no clock.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}
```

### `crates/pf-store/Cargo.toml`

```toml
[package]
name = "pf-store"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
rusqlite = { workspace = true }
rust_decimal = { workspace = true }
jiff = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
log = { workspace = true }

[dev-dependencies]
rust_decimal_macros = { workspace = true }
tempfile = { workspace = true }
pretty_assertions = { workspace = true }
```

### `crates/pf-store/src/lib.rs`

```rust
//! SQLite persistence for the in-memory `Db`.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}
```

### `crates/pf-engine/Cargo.toml`

```toml
[package]
name = "pf-engine"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
pf-core = { workspace = true }
pf-store = { workspace = true }
rust_decimal = { workspace = true }
jiff = { workspace = true }
thiserror = { workspace = true }
log = { workspace = true }

[dev-dependencies]
rust_decimal_macros = { workspace = true }
proptest = { workspace = true }
tempfile = { workspace = true }
pretty_assertions = { workspace = true }
```

### `crates/pf-engine/src/lib.rs`

```rust
//! The engine: apply/undo/redo, app state, and all user commands.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}
```

### `crates/pf-api/Cargo.toml`

```toml
[package]
name = "pf-api"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
pf-core = { workspace = true }
pf-engine = { workspace = true }
tiny_http = { workspace = true }
serde_json = { workspace = true }
rust_decimal = { workspace = true }
jiff = { workspace = true }
thiserror = { workspace = true }
log = { workspace = true }

[dev-dependencies]
rust_decimal_macros = { workspace = true }
insta = { workspace = true }
tempfile = { workspace = true }
pretty_assertions = { workspace = true }
```

### `crates/pf-api/src/lib.rs`

```rust
//! Localhost HTTP API, byte-compatible with the Swift app.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}
```

### `crates/pf-ui/Cargo.toml`

```toml
[package]
name = "pf-ui"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
pf-core = { workspace = true }
pf-engine = { workspace = true }
eframe = { workspace = true }
egui = { workspace = true }
egui_extras = { workspace = true }
egui_plot = { workspace = true }
rfd = { workspace = true }
rust_decimal = { workspace = true }
jiff = { workspace = true }
log = { workspace = true }

[dev-dependencies]
egui_kittest = { workspace = true }
rust_decimal_macros = { workspace = true }
tempfile = { workspace = true }
```

### `crates/pf-ui/src/lib.rs`

```rust
//! The egui user interface.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}

/// Placeholder app; replaced in T13.
#[derive(Default)]
pub struct PfApp;

impl eframe::App for PfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("PFRust");
            ui.label("Scaffold OK");
        });
    }
}
```

### `apps/pf/Cargo.toml`

```toml
[package]
name = "pf"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true
description = "PFRust desktop app"

[lints]
workspace = true

[[bin]]
name = "PF"
path = "src/main.rs"

[dependencies]
pf-ui = { workspace = true }
pf-engine = { workspace = true }
pf-api = { workspace = true }
eframe = { workspace = true }
egui = { workspace = true }
log = { workspace = true }
env_logger = { workspace = true }
anyhow = { workspace = true }
directories = { workspace = true }
```

### `apps/pf/src/main.rs`

```rust
//! PFRust desktop entry point. Wiring of engine and API server happens in T13.

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("PF"),
        ..Default::default()
    };
    eframe::run_native("PF", options, Box::new(|_cc| Ok(Box::new(pf_ui::PfApp))))
}
```

### `apps/pf-cli/Cargo.toml`

```toml
[package]
name = "pf-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true
description = "PFRust command-line tool"

[lints]
workspace = true

[dependencies]
pf-model = { workspace = true }
pf-core = { workspace = true }
pf-engine = { workspace = true }
anyhow = { workspace = true }
directories = { workspace = true }
env_logger = { workspace = true }
log = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
predicates = { workspace = true }
tempfile = { workspace = true }
```

### `apps/pf-cli/src/main.rs`

```rust
//! PFRust CLI. Commands are implemented in T11.

fn main() {
    println!("pf-cli {}", env!("CARGO_PKG_VERSION"));
}
```

### `xtask/Cargo.toml`

```toml
[package]
name = "xtask"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
anyhow = { workspace = true }
regex = { workspace = true }
```

### `xtask/src/main.rs`

```rust
//! Build automation. `rules` and `ci` are implemented in T00b.

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    eprintln!("xtask: unknown task '{task}' (implemented in T00b)");
    std::process::exit(2);
}
```

## Tests

- Each library has a placeholder `scaffold_compiles` test. Later tasks replace these placeholders.

## Done when

```bash
cd PFRust
cargo build --workspace                                  # no warnings
cargo test --workspace                                   # all pass
cargo clippy --workspace --all-targets -- -D warnings    # clean
cargo fmt --all --check                                  # clean
cargo run -p pf                                          # window opens (manual check)
```

## Out of scope

- Any real code. Do not add modules yet.
- Do not add `Cargo.lock` to `.gitignore` (binaries commit their lock file).
