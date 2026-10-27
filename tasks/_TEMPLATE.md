# Txx: <Title>

**Phase:** n · **Deps:** Txx, Tyy · **Crate(s):** `pf-…` · **Size:** small | medium | large (≈ lines of code + tests)

## Goal

One paragraph: what exists after this task that did not exist before, and why it matters.

## Read first

- Docs: `docs/…` (sections)
- Swift: `../PFApp/Sources/PFApp/…swift` (functions)
- Rules: R-…, R-… (`docs/business-rules.md`)

## Deliverables

Exact files to create or modify, with the public API as Rust signatures:

```rust
// crates/pf-core/src/<module>.rs
pub fn …(…) -> …;
```

## Steps

1. Numbered, concrete steps. Each step names files and functions.
2. Tests before implementation where possible.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_xxx_1_…` | R-XXX-1 | … |
| `beh_xxx_01_…` | BEH-XXX-01 | … |

Each row is a test that must exist and pass. Add more if you find edge cases.

## Pitfalls

- Things a small model is likely to get wrong here (API traps, Swift quirks, off-by-one, time zones).

## Done when

```bash
cargo test -p <crate>
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules        # (after T00b): no IDs from this task missing
```

## Out of scope

- What NOT to do in this task (it belongs to Tzz).
