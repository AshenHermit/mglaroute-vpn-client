# CI PR Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GitHub Actions runs `pnpm test` and `cargo test` on every PR into `main` so contributors do not need to run both suites locally.

**Architecture:** New `.github/workflows/ci.yml` with two independent parallel jobs on `windows-latest`: `test-js` and `test-rust`. Triggers are `pull_request` to `main` and `push` to `main`. Rust job uses the test-only `TAURI_CONFIG` override and single-threaded tests; pnpm/Rust caches keep repeated runs fast. `release.yml` untouched.

**Tech Stack:** GitHub Actions, pnpm 9, Node 20, Nx, Vitest, Rust stable, Tauri v2, `Swatinem/rust-cache@v2`.

**Spec:** `docs/superpowers/specs/2026-11-27-ci-pr-tests-design.md`

## Global Constraints

- `windows-latest` runner for both jobs.
- `NX_DAEMON: 'false'` on the JS job.
- `TAURI_CONFIG: '{"bundle":{"externalBin":[],"resources":[]}}'` on the Rust job.
- Rust command: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`.
- pnpm 9 via `pnpm/action-setup@v4`; Node 20 via `actions/setup-node@v4` with `cache: pnpm`.
- `actions/checkout@v4`; `dtolnay/rust-toolchain@stable`; `Swatinem/rust-cache@v2` scoped to `apps/desktop/src-tauri -> target`.
- `pnpm install --frozen-lockfile` before suites.
- No installer, signing, binary fetch, publishing or branch-protection changes in this plan.

## Review Focus

- Fork PR / read-only token: no secrets or write permissions in the workflow — verified by absence of `permissions:` and secrets in Task 1's file.
- Clean checkout has no bundled xray: `cargo test` must not fail on `externalBin` — pinned by `TAURI_CONFIG` in Task 1 and observed in Task 3.
- Process-global `EVENT_SINK` makes parallel Rust tests flaky — pinned by `--test-threads=1` in Task 1 and observed in Task 3.
- Nx daemon flake can stall/hang the JS job — pinned by `NX_DAEMON: 'false'` in Task 1 and observed in Task 3.
- Lockfile drift or wrong pnpm version breaks install — pinned by `pnpm/action-setup@v4` version 9 plus `--frozen-lockfile` in Task 1 and observed in Task 3.
- Never claim green before seeing the run — Task 3 requires `gh pr checks` output.

---

### Task 1: Add the CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: root `package.json` scripts `test`; `apps/desktop/src-tauri/Cargo.toml`; `pnpm-lock.yaml`.
- Produces: GitHub check names `test-js` and `test-rust`, both required to pass before a PR can be considered verified.

- [ ] **Step 1: Create `.github/workflows/ci.yml` with exactly this content**

```yaml
name: ci

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

jobs:
  test-js:
    runs-on: windows-latest
    env:
      NX_DAEMON: 'false'
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm test

  test-rust:
    runs-on: windows-latest
    env:
      TAURI_CONFIG: '{"bundle":{"externalBin":[],"resources":[]}}'
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: apps/desktop/src-tauri -> target
      - run: pnpm install --frozen-lockfile
      - run: cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1
```

- [ ] **Step 2: Verify local syntax and commit**

Run: `git diff --check && git add .github/workflows/ci.yml && git commit -m "ci: run pnpm and cargo tests on pull requests"`
Expected: clean diff check; commit created.

### Task 2: Document CI as the verification path

**Files:**
- Modify: `AGENTS.md` (Verification section)

**Interfaces:**
- Consumes: check names from Task 1 (`test-js`, `test-rust`).
- Produces: updated local verification notes pointing to CI.

- [ ] **Step 1: In `AGENTS.md` Verification section, add a leading CI note**

Insert before the `pnpm test` bullet, as two lines:

```markdown
- GitHub Actions is the authoritative check on PRs into `main` (`.github/workflows/ci.yml`): jobs `test-js` (`pnpm test`) and `test-rust` (`cargo test` with the test-only `TAURI_CONFIG` override). Both must be green; local runs are optional.
```

Keep all existing bullets unchanged.

- [ ] **Step 2: Commit**

Run: `git add AGENTS.md && git commit -m "docs: point verification at PR CI"`
Expected: commit created.

### Task 3: Observe the checks on PR #3

**Files:** none (verification only)

**Interfaces:**
- Consumes: workflow from Task 1 pushed to `fix/subprocesses`; PR #3 (`fix/subprocesses` → `main`).

- [ ] **Step 1: Push the branch and watch checks**

Run: `git push origin fix/subprocesses` then `gh pr checks 3 --watch --fail-fast`
Expected: `test-js` and `test-rust` both pass. If either fails, read `gh run view --log-failed <run-id>` and fix before claiming success.

- [ ] **Step 2: Record result**

Run: `gh pr checks 3`
Expected: both checks green; report status to partner.
