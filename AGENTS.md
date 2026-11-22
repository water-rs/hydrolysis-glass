# AGENTS.md

Rules for working in `water-rs/hydrolysis-glass`.

## Clean-room rule and visual parity

- Development is strictly clean-room. Never mention proprietary vendor copyrights, trademarks, product names, or framework names in code, comments, shaders, documentation, commit messages, issues, or pull requests, and never include proprietary vendor assets, shaders, or binaries.
- Visual parity is calibrated strictly against `waterui-apple` (WaterUI's native backend) on macOS 27 (`#81`). Close visual divergences through clean-room specification (`docs/specification.md`, tagging every value `measured`, `fitted`, or `chosen`) and independent implementation verified via offscreen rendering and FLIP perceptual metrics.

## Architecture and hygiene

- Express chrome and glass surfaces through `cherenkov-record` and Hydrolysis's public widget theme contract; keep `verification` GPU-to-CPU readback gated behind the `verification` feature so consumers never build readback paths by default.
- Do not use crate- or file-level `#![allow]` or `#![expect]`; every item-level `#[allow(...)]` must carry a `reason` (`tests/lint_hygiene.rs`).
- Keep `Cargo.lock` committed and `.gitignore` limited to `/target`. Every dependency declares `default-features = false` with only the needed features, matching `water-rs/waterui`'s `[patch.crates-io]` table when riding framework `main`.
- Never open visible GUI windows on the developer MacBook Pro; verify rendering headless via `tests/gpu.rs` and `tests/hydrolysis.rs`.

## GitHub workflow and checks

- Finding a problem creates a GitHub issue (`gh issue create`); solving one lands via an isolated worktree on a topic branch off `origin/main` (never commit or push directly to `main`), with GPG-signed commits (`git commit -S`) and a pull request linked with `Fixes #N` (or `Part of #1`).
- Verify changes before pushing with:
  - `cargo fmt --all -- --check`
  - `cargo clippy --locked --all-targets -- -D warnings`
  - `cargo nextest run --locked` and `cargo test --locked --doc`
  - `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps`
  - MSRV check (`cargo check --locked` with the `rust-version` declared in `Cargo.toml`)
