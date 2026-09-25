# Development Environment

## Pinned Rust toolchain

The workspace uses Rust `1.90.0`, edition 2024, Cargo resolver 3, `rustfmt`, and Clippy. The toolchain must already be provisioned by the development image or CI runner. Repository commands never install or update it.

Verify the local environment before building:

```sh
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
```

## Offline dependency policy

Cargo is permanently offline through `.cargo/config.toml`. Do not override that setting and do not add a network fallback. Every third-party dependency must be available in the approved cache, represented in `Cargo.lock`, and declared with `default-features = false` before it can enter the workspace.

Run the deterministic offline gate from the repository root:

```sh
scripts/check-offline.sh
```

The gate runs locked, offline metadata resolution, build, and tests. If the pinned toolchain or a locked crate is unavailable, it exits with code 2 and prints `BLOCKED_OFFLINE_DEPENDENCY` plus the exact missing toolchain package or crates Cargo reported. Resolve that condition by updating the approved development image or dependency cache outside the release job. Never repair it by downloading during CI or release.

## Workspace commands

Run the complete local gate with the pinned toolchain already available:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo build --workspace --locked --offline
cargo test --workspace --locked --offline
scripts/check-offline.sh
```

Create and register a package manifest in `workspace.members` before running any package-scoped Cargo command. Packages start with no default features. Dependencies must use this form:

```toml
[dependencies]
example = { version = "1", default-features = false }
```
