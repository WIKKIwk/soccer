# FND-005 — Rust toolchain policy

**Status:** Accepted  
**Decision date:** 2026-08-08

## Verified baseline

The current development environment reports:

```text
rustc 1.96.1 (31fca3adb 2026-06-26)
cargo 1.96.1 (356927216 2026-06-26)
host: aarch64-apple-darwin
active channel: stable
```

The required `rustfmt` and `clippy` components are installed for the active stable toolchain.

## Repository policy

- The project uses the Rust stable channel only.
- Nightly, beta, compiler forks, and `#![feature(...)]` unstable features are not allowed in the first-year core.
- The repository declares the channel and required components in `rust-toolchain.toml`.
- The minimal rustup profile is sufficient; documentation and unrelated components are not required for builds.
- All workspace crates use the same toolchain and edition policy; a crate must not override it silently.
- Toolchain upgrades are explicit changes: update the toolchain file, run the full verification hierarchy, inspect dependency/MSRV effects, and record the result.
- CI and release preparation must use the repository toolchain file rather than the developer machine’s default compiler.

## Verification commands

The baseline workspace checks are:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

Focused tests run before the wider workspace checks, following the roadmap verification hierarchy.

## Compatibility policy

Dependencies must support the selected stable toolchain. A dependency that requires nightly or an unexplained compiler override is rejected. The project does not silently lower compiler policy to accommodate a speculative dependency.

The current `Cargo.toml` files use Rust edition 2024 and the verified active stable compiler. An explicit minimum supported Rust version may be added once the first dependency and CI baseline is frozen; until then, the repository toolchain file is the development and verification authority.

## Acceptance criteria

FND-005 is complete when the repository declares one stable toolchain policy, required formatting/lint components are known, upgrades are explicit, and local/CI commands can select the same channel without hidden nightly behavior.

