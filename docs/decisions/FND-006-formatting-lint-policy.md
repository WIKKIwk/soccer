# FND-006 — Formatting and lint policy

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [FND-005 Rust toolchain policy](FND-005-rust-toolchain-policy.md)

## Formatting

- All Rust source is formatted by `cargo fmt --all` using the repository `rustfmt.toml`.
- Formatting uses Rust edition 2024 and Unix line endings.
- Manual formatting exceptions are not accepted; generated files are excluded from source review and must not be checked in.
- A change is not complete while `cargo fmt --all -- --check` fails.

## Linting

The required workspace lint command is:

```text
cargo clippy --workspace --all-targets -- -D warnings
```

This applies to libraries, binaries, unit-test targets, and integration-test targets. Every compiler or Clippy warning is a verification failure.

Crate-level `#![forbid(unsafe_code)]` remains mandatory. New `#[allow(...)]` attributes require a narrow scope and an adjacent explanation of why the lint is safe to suppress; broad crate/workspace lint suppression is forbidden.

## Verification order

The normal local order follows the roadmap:

1. focused changed-module test;
2. changed-crate tests;
3. `cargo test --workspace`;
4. `cargo clippy --workspace --all-targets -- -D warnings`;
5. `cargo fmt --all -- --check`;
6. `cargo build --workspace` when build output is relevant.

Formatting and lint checks use the repository-selected stable toolchain from `rust-toolchain.toml`. The same commands are the future CI baseline.

## Review rules

- No warning is hidden to make a task pass.
- No lint policy is weakened for tests.
- Dead code, fake fallback paths, unused abstractions, and generated build output are not accepted as implementation shortcuts.
- A lint exception is reviewed with the owning module and removed when the underlying reason no longer exists.

## Acceptance criteria

FND-006 is complete when formatting has a repository config, linting has one strict workspace command, unsafe code is forbidden, warnings fail verification, and the policy passes against the current workspace.

