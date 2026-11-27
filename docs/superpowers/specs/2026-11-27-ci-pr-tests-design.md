# CI: tests on pull requests — design

Status: proposed

## Intent

Contributor should not need to run `pnpm test` and `cargo test` locally before
opening a PR: GitHub Actions runs both on every PR into `main` (and on pushes
to `main` for a green baseline) and reports the result on the PR.

## Goals

- Run the same two suites that are currently the local verification contract:
  `pnpm test` (Nx contracts + desktop, then `vitest run scripts`) and
  `cargo test` (Tauri unit + integration + doc tests).
- Fail the PR check on any failing suite.
- Keep both suites independent and visibly reported.

## Non-goals

- No build/installer/signing step; that stays in `release.yml`.
- No deployment, no artifact publishing, no branch-protection changes in this pass.
- No attempt to run a real xray binary or `binaries:verify` (no bundled binary in a
  clean checkout).

## Constraints

- `windows-latest` matches the verified local environment and the release job.
- `NX_DAEMON=false` avoids the known Nx daemon flake.
- Plain `cargo test` needs a bundled `externalBin` that CI does not have; use the
  test-only `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` override.
- `EVENT_SINK` is process-global in `state.rs`; Rust tests need `-- --test-threads=1`.
- Rust first compile is slow (~12 min locally); cache Rust artifacts.
- pnpm 9 + Node 20 setup mirrors `release.yml`.

## Approaches

1. **New `ci.yml`, two parallel jobs** (chosen). `test-js` and `test-rust` both
   on `windows-latest`. Triggers: `pull_request` to `main`, `push` to `main`.
   pnpm cache via `setup-node@v4`, Rust cache via `Swatinem/rust-cache@v2`.
   Duplicates a few setup steps but gives independent statuses and shorter wall
   time.
2. **Single job, sequential steps.** Less YAML, but JS failure hides Rust status
   and Rust compile extends total time.
3. **Extend `release.yml` / `workflow_call`.** Rejected: release job is tag-gated,
   fetches and verifies binaries, signs and publishes NSIS — all irrelevant and
   risky for a test check.

## Decisions (builder's calls)

- Use `actions/checkout@v4`, `pnpm/action-setup@v4` (version 9),
  `actions/setup-node@v4` (node 20, `cache: pnpm`), `dtolnay/rust-toolchain@stable`.
- `test-js`: `env: NX_DAEMON: false`, `run: pnpm install --frozen-lockfile` then
  `run: pnpm test`.
- `test-rust`: install pnpm/Node first because Tauri's build script reads the
  frontend config, then `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
  -- --test-threads=1` with the test-only `TAURI_CONFIG` env var.
- No `permissions` block needed (default read-only token is enough).
- Update `AGENTS.md` Verification section to point at CI as the authoritative
  check and note the test-only cargo env.

## Left to builder

- Create `.github/workflows/ci.yml` exactly as decided.
- Update `AGENTS.md` verification notes.
- Verify with `gh pr checks` / `gh run view` once the workflow lands on the PR;
  do not claim green before observing the run.
