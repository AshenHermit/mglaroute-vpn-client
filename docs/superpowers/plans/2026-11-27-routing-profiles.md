# Routing profiles (v2rayN-style) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SQLite-backed routing profiles with ordered domain/IP rules and proxy/direct/block outbounds to the desktop Settings UI, applied through the live connection config.

**Architecture:** Two normalized SQLite tables (`routing_profiles`, `routing_rules`) exposed by ten Tauri commands with camelCase payloads. `build_connection_config` gains active rules and an asset-dir argument; private bypass and proxy exclusions keep priority, user rules follow in order, catch-all proxy stays last. A two-pane Routing tab in Settings manages everything locally; `geosite.dat`/`geoip.dat` are fetched, hashed, bundled, and located via `XRAY_LOCATION_ASSET`.

**Tech Stack:** Tauri v2, Rust/`rusqlite`/`serde_json`, React 18, TypeScript strict, Zod, Vitest, Tailwind 3 + local shadcn-style Radix wrappers, pnpm/Nx.

**Spec:** `docs/superpowers/specs/2026-11-27-routing-profiles-design.md` (status `accepted`). Read it alongside this plan.

## Global Constraints

- Work only in worktree `D:\Users\hermit\vpn\mglaroute-vpn-client\.worktrees\routing-profiles` on branch `feature/routing-profiles`.
- Stack fixed: Tauri v2 + Rust/`rusqlite`, React 18 + TypeScript strict, shadcn-style local Radix wrappers + Tailwind 3, Zustand 5 with `persist` only for non-secret UI prefs. Durable data lives in SQLite.
- Existing schema is `PRAGMA user_version = 2`; `storage/mod.rs::migrate` only has v1 → v2 (`test_concurrency`). Use `CREATE TABLE IF NOT EXISTS` in `schema.sql` without a version bump; verify with a fresh-DB test.
- `ConfigSnapshot` is the immutable input for one xray process. Config generation must stay deterministic for identical inputs.
- `lib.rs::command_list!` registers every Tauri command, and `commands/mod.rs::command_names_registered()` asserts the sorted command-name surface; add all ten new names there.
- Rust emits `camelCase` payloads; Zod contracts in `packages/contracts/src/schemas.ts` must match exactly.
- Probe config stays untouched: `build_probe_config` must not read routing state, private bypass, exclusions, or geo assets.
- Only an enabled + active profile's enabled rules may reach the live config.
- Private bypass (`XRAY_BYPASS_IPS`) → `direct` always precedes user rules; catch-all `inboundTag:["proxy-in"]` → `proxy-out` always stays last.
- `block` means xray outbound `{ "tag": "block", "protocol": "blackhole", "settings": {} }`.
- No `PRAGMA foreign_keys` is enabled anywhere — never rely on `ON DELETE CASCADE`; delete child rules explicitly in a transaction.
- Rust-side validation only; TS may validate for UX but Rust is authoritative.
- Caveman-mode terse output; preserve exact identifiers.
- Baseline gate: `pnpm test` is green (130 desktop + 8 contracts + 24 scripts, 0 failures). `cargo test` with test-only `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` and `-- --test-threads=1` is still running and must be green before task implementation starts.

## Review Focus

These are the inputs/failure modes most likely to bite a user that no happy-path test alone catches. Each is pinned in the owning task.

1. **Active flag goes stale** — disabling an active profile, or removing it, must leave no active rules in config. Tests: `only_one_active_profile_and_disabled_cannot_activate`, `remove_profile_deletes_rules`, `rules_order_and_active_filter` (Task 1), `build_connection_config_no_active_rules_unchanged` (Task 3).
2. **Block rule without blackhole outbound** — a `block` rule must produce a usable config, and a config with no block rules must not carry an unused outbound. Tests: `build_connection_config_block_rule_adds_blackhole`, `build_connection_config_no_block_rule_omits_blackhole` (Task 3).
3. **Prefixed entries reach xray assets** — `geosite:`/`geoip:` must stay verbatim in rule arrays and the bundled `.dat` files must be reachable. Tests: `build_connection_config_preserves_geo_prefixes_and_sets_asset_env` (Task 3), `verifyBinaries fails when a geo asset is missing`, `bundles geoip.dat and geosite.dat as Tauri resources` (Task 4).
4. **Probe contamination** — a probe must ignore all routing state. Test: `build_probe_config_unchanged_with_routing_rules` (Task 3, assert exact single-rule proxy shape).
5. **Ordering and per-rule toggle** — first-match order and enable/disable/reorder must be deterministic. Tests: `rules_order_and_active_filter`, `reorder_rule_swaps_neighbors_and_clamps` (Task 1), `build_connection_config_rule_order_and_disabled_rule_skipped` (Task 3).

---

## File Structure

| File | Responsibility |
| --- | --- |
| `apps/desktop/src-tauri/src/storage/schema.sql` | Add `routing_profiles` / `routing_rules` tables + indexes. |
| `apps/desktop/src-tauri/src/storage/models.rs` | Add `RoutingProfile`, `RoutingRule` row structs with `Default`. |
| `apps/desktop/src-tauri/src/storage/mod.rs` | Register `routing_repo` module. |
| `apps/desktop/src-tauri/src/storage/repos.rs` | Make `ts_string`, `read_ts` visible to `routing_repo`. |
| `apps/desktop/src-tauri/src/storage/routing_repo.rs` | New: all profile/rule SQL. |
| `apps/desktop/src-tauri/src/commands/routing.rs` | New: ten Tauri commands + `_impl` fns + validation. |
| `apps/desktop/src-tauri/src/commands/mod.rs` | Re-export routing commands, add `asset_dir`, extend command-surface test. |
| `apps/desktop/src-tauri/src/commands/connection.rs` | Load active rules and pass to config builder. |
| `apps/desktop/src-tauri/src/core/config.rs` | Generate routing rules from active rules; conditional block outbound/env. |
| `apps/desktop/src-tauri/src/lib.rs` | Register commands; pass resource-dir/binaries into `bootstrap`. |
| `scripts/xray-assets.mjs` | Export `GEO_ASSETS`. |
| `scripts/fetch-xray.mjs` | Extract/copy/verify `geoip.dat`, `geosite.dat`. |
| `scripts/fetch-xray.test.mjs` | Geo asset fixtures and verification tests. |
| `scripts/release-config.test.mjs` | Assert geo resources in `tauri.conf.json`. |
| `apps/desktop/src-tauri/tauri.conf.json` | Bundle staged `.dat` files. |
| `packages/contracts/src/schemas.ts` | Add `RoutingProfileSchema`, `RoutingRuleSchema`. |
| `packages/contracts/src/schemas.test.ts` | Contract parse/reject tests. |
| `apps/desktop/src/pages/Settings.tsx` | Add `routing` tab, `grid-cols-5`. |
| `apps/desktop/src/pages/settings-page.test.tsx` | Update tab count/column assertions. |
| `apps/desktop/src/features/routing-editor/RoutingEditor.tsx` | New: master/detail UI. |
| `apps/desktop/src/features/routing-editor/routing-editor.test.tsx` | New: component behavior test. |

---

### Task 1: Storage schema, models, and repository

**Files:**
- Modify: `apps/desktop/src-tauri/src/storage/schema.sql`
- Modify: `apps/desktop/src-tauri/src/storage/models.rs`
- Modify: `apps/desktop/src-tauri/src/storage/mod.rs`
- Modify: `apps/desktop/src-tauri/src/storage/repos.rs`
- Create: `apps/desktop/src-tauri/src/storage/routing_repo.rs`

**Interfaces:**
- Consumes: existing `storage::migrate(conn: &Connection)`, `repos::ts_string`, `repos::read_ts`, `chrono::DateTime<Utc>` convention.
- Produces:
  - `models::RoutingProfile { id: String, name: String, enabled: bool, is_active: bool, sort: i64, created_at: DateTime<Utc>, updated_at: DateTime<Utc> }`
  - `models::RoutingRule { id: String, profile_id: String, remark: String, enabled: bool, outbound: String, domains: String, ips: String, sort: i64, created_at: DateTime<Utc>, updated_at: DateTime<Utc> }`
  - `routing_repo` functions (every one takes `conn: &Connection` first and returns `rusqlite::Result<_>`; `bool` results mean `Result<bool>`):
    - `insert_profile(&RoutingProfile) -> Result<()>`, `get_profile(&str) -> Result<Option<RoutingProfile>>`, `list_profiles() -> Result<Vec<RoutingProfile>>`, `get_active_profile() -> Result<Option<RoutingProfile>>`
    - `exists_profile_by_name(name: &str, except_id: Option<&str>) -> Result<bool>`
    - `next_profile_sort() -> Result<i64>`
    - `update_profile(id: &str, name: &str, enabled: bool) -> Result<()>` — clearing `enabled` also clears `is_active`
    - `activate_profile(id: &str) -> Result<bool>` — `Ok(false)` when missing or disabled
    - `remove_profile(id: &str) -> Result<()>` — deletes rules then profile in one transaction
    - `insert_rule(&RoutingRule) -> Result<()>`, `get_rule(&str) -> Result<Option<RoutingRule>>`, `list_rules(profile_id: &str) -> Result<Vec<RoutingRule>>`
    - `list_active_rules() -> Result<Vec<RoutingRule>>` — enabled rules of enabled + active profile, ordered `sort ASC, id ASC`
    - `next_rule_sort(profile_id: &str) -> Result<i64>`
    - `update_rule(id: &str, remark: &str, enabled: bool, outbound: &str, domains: &[String], ips: &[String]) -> Result<()>`
    - `remove_rule(id: &str) -> Result<()>`, `reorder_rule(id: &str, direction: i32) -> Result<()>` — `direction` is `-1` or `1`, clamps at edges

- [ ] **Step 1: Write failing repository tests**

In `storage/routing_repo.rs` add `#[cfg(test)] mod tests` with a helper `fn fresh_db() -> Connection` that calls `crate::storage::migrate(&conn)` (plus `ensure_defaults` if required by `migrate`; match the existing test helper in `repos.rs`). Write these tests:

```rust
#[test]
fn fresh_db_creates_routing_tables() {
    let conn = fresh_db();
    // assert sqlite_master has routing_profiles and routing_rules
    // PRAGMA user_version still equals 2
}

#[test]
fn profile_crud_and_unique_name() {
    // insert two profiles, list order by sort/id, get by id,
    // exists_profile_by_name("x", None) true,
    // exists_profile_by_name("x", Some(existing_id)) false for same row,
    // rename, remove, get returns None
}

#[test]
fn only_one_active_profile_and_disabled_cannot_activate() {
    // create p1, p2; activate p1 => is_active; activate p2 => p1 cleared
    // disable p1 (is_active cleared), then activate p1 => false
    // update p2 enabled=false clears its active flag
    // remove the active profile => get_active_profile() is None
}

#[test]
fn remove_profile_deletes_rules() {
    // profile + 2 rules; remove_profile; get_rule for both None
}

#[test]
fn rules_order_and_active_filter() {
    // two profiles p1(active,enabled), p2(disabled)
    // rules with sort 2,1 in p1 and one enabled rule in p2
    // list_rules(p1) ordered by sort ASC,id ASC
    // list_active_rules() returns only p1's enabled rules, ordered
    // disabled rule in p1 excluded
}

#[test]
fn reorder_rule_swaps_neighbors_and_clamps() {
    // sorts [10,20,30]; reorder_rule(middle,-1) => [10(middle),20(first),30]
    // reorder_rule(first,-1) and reorder_rule(last,1) leave order unchanged
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml routing_repo -- --test-threads=1`
Expected: FAIL — `routing_repo` module and structs do not exist yet.

- [ ] **Step 3: Add schema and models**

In `schema.sql`, after existing tables, add (keeping existing `strftime('%Y-%m-%dT%H:%M:%fZ','now')` timestamp convention):

```sql
CREATE TABLE IF NOT EXISTS routing_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  enabled INTEGER NOT NULL DEFAULT 1,
  is_active INTEGER NOT NULL DEFAULT 0,
  sort INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_routing_profiles_active ON routing_profiles(is_active, enabled);
CREATE TABLE IF NOT EXISTS routing_rules (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL REFERENCES routing_profiles(id) ON DELETE CASCADE,
  remark TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 1,
  outbound TEXT NOT NULL CHECK (outbound IN ('proxy','direct','block')),
  domains TEXT NOT NULL DEFAULT '[]',
  ips TEXT NOT NULL DEFAULT '[]',
  sort INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_routing_rules_profile_sort ON routing_rules(profile_id, sort, id);
```

Do not change `PRAGMA user_version`. In `models.rs`, add the two structs above with `#[derive(Debug, Clone, Serialize, Deserialize)]` (or match existing derives) and `Default` impls. In `mod.rs`, add `pub mod routing_repo;`. In `repos.rs`, change `fn ts_string` and `fn read_ts` to `pub(crate)`.

- [ ] **Step 4: Implement `routing_repo.rs`**

Implement every signature listed in Interfaces. Use `serde_json` for `domains`/`ips` text. `activate_profile` must: check `enabled = 1` first; then in one transaction `UPDATE routing_profiles SET is_active=0`, `UPDATE ... SET is_active=1, updated_at=... WHERE id=?`, returning whether a row was updated. `update_profile` clears `is_active` when `enabled` is false. `remove_profile` deletes `routing_rules` rows for that profile before deleting the profile. `reorder_rule` resolves the neighbor via `ORDER BY sort, id`, swaps `sort` values, and no-ops at edges.

- [ ] **Step 5: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml routing_repo -- --test-threads=1`
Expected: PASS — 6 tests, 0 failures.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/storage/schema.sql apps/desktop/src-tauri/src/storage/models.rs apps/desktop/src-tauri/src/storage/mod.rs apps/desktop/src-tauri/src/storage/repos.rs apps/desktop/src-tauri/src/storage/routing_repo.rs
git commit -m "feat: add routing profile storage"
```

### Task 2: Routing commands, validation, and command surface

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/routing.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `apps/desktop/src-tauri/src/commands/routing.rs`, `apps/desktop/src-tauri/src/commands/mod.rs`

**Interfaces:**
- Consumes: all Task 1 repo functions and `RoutingProfile` / `RoutingRule`.
- Produces:
  - `RoutingProfilePayload { id: String, name: String, enabled: bool, is_active: bool, sort: i64, created_at: DateTime<Utc>, updated_at: DateTime<Utc> }` with `#[derive(Serialize)] #[serde(rename_all = "camelCase")]` and `From<RoutingProfile>`.
  - `RoutingRulePayload { id: String, profile_id: String, remark: String, enabled: bool, outbound: String, domains: Vec<String>, ips: Vec<String>, sort: i64, created_at: DateTime<Utc>, updated_at: DateTime<Utc> }` with same attrs and `From<RoutingRule>` (parses JSON text to `Vec<String>`).
  - `_impl` functions taking `&AppState`:
    - `profile_list_impl(state) -> Result<Vec<RoutingProfilePayload>, CommandError>`
    - `profile_create_impl(state, name: String) -> Result<RoutingProfilePayload, CommandError>`
    - `profile_update_impl(state, id: String, name: String, enabled: bool) -> Result<RoutingProfilePayload, CommandError>`
    - `profile_activate_impl(state, id: String) -> Result<RoutingProfilePayload, CommandError>`
    - `profile_remove_impl(state, id: String) -> Result<(), CommandError>`
    - `rule_list_impl(state, profile_id: String) -> Result<Vec<RoutingRulePayload>, CommandError>`
    - `rule_create_impl(state, profile_id: String, remark: String, enabled: bool, outbound: String, domains: Vec<String>, ips: Vec<String>) -> Result<RoutingRulePayload, CommandError>`
    - `rule_update_impl(state, id: String, remark: String, enabled: bool, outbound: String, domains: Vec<String>, ips: Vec<String>) -> Result<RoutingRulePayload, CommandError>`
    - `rule_remove_impl(state, id: String) -> Result<(), CommandError>`
    - `rule_reorder_impl(state, id: String, direction: i32) -> Result<(), CommandError>`
  - Ten `#[tauri::command]` wrappers named exactly: `routing_profile_list`, `routing_profile_create`, `routing_profile_update`, `routing_profile_activate`, `routing_profile_remove`, `routing_rule_list`, `routing_rule_create`, `routing_rule_update`, `routing_rule_remove`, `routing_rule_reorder`.

- [ ] **Step 1: Write failing command tests**

In `commands/routing.rs`, using the existing test `AppState` constructor from `commands/mod.rs` tests, write:

```rust
#[test]
fn payloads_are_camel_case() {
    // serde_json::to_value(&RoutingProfilePayload) has keys id,name,enabled,isActive,sort,createdAt,updatedAt
    // RoutingRulePayload has profileId, domains, ips, createdAt, updatedAt
}

#[test]
fn profile_crud_commands() {
    // create -> list contains it -> update name/enabled -> activate -> list has exactly one isActive
    // remove -> list no longer contains it
}

#[test]
fn profile_validation_errors() {
    // empty/whitespace name => InvalidInput
    // duplicate name => InvalidInput
    // activate missing id => NotFound
    // activate disabled profile => InvalidInput
}

#[test]
fn rule_crud_and_validation() {
    // create with outbound "block", domains ["geosite:private"], ips ["geoip:private","10.0.0.0/8"]
    // list returns same values; update toggles enabled and outbound; reorder moves rule
    // invalid outbound "tunnel" => InvalidInput
    // domains=["  "], ips=[] => InvalidInput (blank-only)
    // domains=[], ips=[] => InvalidInput (no matchers)
    // create with missing profile_id => NotFound
}

#[test]
fn mutation_commands_require_permit() {
    // hold ConnectionGuard, assert profile_create returns busy code; drop guard, retry succeeds
}
```

In `commands/mod.rs::command_names_registered()`, add the ten names to the expected sorted vector so the test fails until registration is done.

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml routing -- --test-threads=1`
Expected: FAIL — command module and payloads missing; command-surface assertion lists names not registered.

- [ ] **Step 3: Implement commands and validation**

Create `commands/routing.rs` following `commands/service_profile.rs`: sha256 IDs prefixed `rtp-` / `rtr-`, `state.permit()` on every mutation, `CommandError::InvalidInput` / `NotFound`. Validation rules: trim profile name (reject empty), unique name case-sensitive via `exists_profile_by_name`, `outbound` must be one of `proxy|direct|block`, trim every domain/IP entry and reject if all entries blank, require at least one non-blank matcher across both arrays, reject unknown `profile_id` / rule `id`. Do not rewrite `geosite:` / `geoip:` prefixes.

Re-export all ten wrapper functions in `commands/mod.rs` (`pub mod routing; pub use routing::{...};`) and add the ten names to `lib.rs::command_list!`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml routing -- --test-threads=1`
Expected: PASS — command tests including `command_names_registered`.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/routing.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: add routing commands"
```

### Task 3: Config generation and connect wiring

**Files:**
- Modify: `apps/desktop/src-tauri/src/core/config.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/connection.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `apps/desktop/src-tauri/src/core/config.rs`

**Interfaces:**
- Consumes: `storage::models::RoutingRule`, `routing_repo::list_active_rules(&conn)`, `AppStateInner::asset_dir: Option<PathBuf>`.
- Produces: `build_connection_config(snapshot: &ConfigSnapshot, exclusions: &[String], rules: &[RoutingRule], asset_dir: Option<&Path>) -> serde_json::Value`. `build_probe_config(profile, port)` unchanged.

- [ ] **Step 1: Write failing config tests**

In `core/config.rs::tests`, add:

```rust
#[test]
fn build_connection_config_no_active_rules_unchanged() {
    // call with &[], None; assert 2 outbounds, private bypass first, one exclusion rule, catch-all last
}

#[test]
fn build_connection_config_rule_order_and_disabled_rule_skipped() {
    // rules: proxy domain, block ip, disabled direct domain
    // assert generated user rules in original sort order, disabled absent
    // assert private bypass and exclusions still precede them, catch-all last
}

#[test]
fn build_connection_config_block_rule_adds_blackhole() {
    // outbound tagged block has protocol blackhole and settings {}
    // user rule with outbound "block" has outboundTag "block"
}

#[test]
fn build_connection_config_no_block_rule_omits_blackhole() {
    // no block rules => no outbound tagged block
}

#[test]
fn build_connection_config_preserves_geo_prefixes_and_sets_asset_env() {
    // domains ["geosite:private"], ips ["geoip:private"]
    // assert exact strings in generated rule arrays
    // asset_dir Some(Path::new("C:/fake/binaries")) => config["env"]["XRAY_LOCATION_ASSET"] == that string
    // asset_dir None => config has no "env" key
}

#[test]
fn build_probe_config_unchanged_with_routing_rules() {
    // build_probe_config(&vless_profile(), 1080) has exactly one proxy outbound, one catch-all proxy rule, no block, no env
}
```

Update all seven existing `build_connection_config(&snapshot, &[...])` call sites (~lines 539, 579, 632, 672, 700, 717, 734) to pass `&[], None`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml config::tests -- --test-threads=1`
Expected: FAIL — new signature and behavior not implemented.

- [ ] **Step 3: Implement config generation**

Change signature as in Interfaces. For each `rule` in `rules` where `rule.enabled`:
- emit one rule with `"field":"domain","domain":[trimmed non-empty domains]` when domains present, `outboundTag` = mapped outbound (`proxy`→`proxy-out`, `direct`→`direct`, `block`→`block`);
- emit one rule with `"field":"ip","ip":[trimmed non-empty ips]` when IPs present.
Preserve entry strings verbatim apart from trimming empty entries. Add `block` outbound only when at least one enabled rule maps to it. Add `"env":{"XRAY_LOCATION_ASSET": <asset_dir string>}` only when `asset_dir` is `Some`. Keep `routing()` `domainStrategy: AsIs`, keep private bypass and exclusions before user rules, keep catch-all last. Do not touch `build_probe_config`.

- [ ] **Step 4: Wire active rules and asset dir**

In `commands/connection.rs::connect_impl`, after loading `exclusions`, call `routing_repo::list_active_rules(&conn)` on the same connection and pass `&rules` plus `state.inner.asset_dir.as_deref()` to `build_connection_config`. In `commands/mod.rs`, add `asset_dir: Option<PathBuf>` to `AppStateInner`; `AppState::new` / `new_with_job` set it to `None`; change `bootstrap(dir: &Path, asset_dir: Option<PathBuf>)`. In `lib.rs::run`, call `bootstrap(&dir, app.path().resource_dir().ok().map(|p| p.join("binaries")))` — bundled dat files land in `resource_dir/binaries` via `bundle.resources`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: PASS — full Rust suite, 0 failures. Task 1/2 tests included.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/core/config.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/commands/connection.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: apply routing rules in connection config"
```

### Task 4: Geo asset fetch, verification, and bundling

**Files:**
- Modify: `scripts/xray-assets.mjs`
- Modify: `scripts/fetch-xray.mjs`
- Modify: `scripts/fetch-xray.test.mjs`
- Modify: `scripts/release-config.test.mjs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: existing `fetchBinary()` zip verification path, `verifyBinaries(options)`, `selectAsset`, `pinnedRelease`.
- Produces: `GEO_ASSETS = ["geoip.dat", "geosite.dat"]`; extracted `.dat` files + `.sha256` sidecars in staged binaries dir; `bundle.resources` entries `binaries/geoip.dat`, `binaries/geosite.dat`.

- [ ] **Step 1: Write failing script tests**

In `scripts/fetch-xray.test.mjs`, extend the fixture zip to include `geoip.dat`, `geosite.dat`, `geoip.dat.sha256`, `geosite.dat.sha256` with correct digests; add:

```js
it("copies geo assets and sidecars from the verified zip", async () => { ... });
it("verifyBinaries fails when a geo asset is missing", async () => { ... });
```

Keep existing dev-override and binary-sha-mismatch tests. In `scripts/release-config.test.mjs`, add:

```js
it("bundles geoip.dat and geosite.dat as Tauri resources", () => {
  // expect bundle.resources to contain "binaries/geoip.dat" and "binaries/geosite.dat"
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run scripts/fetch-xray.test.mjs scripts/release-config.test.mjs`
Expected: FAIL — geo files not extracted/verified; resources not listed.

- [ ] **Step 3: Implement geo asset handling**

In `scripts/xray-assets.mjs`, add `export const GEO_ASSETS = ["geoip.dat", "geosite.dat"];`. In `scripts/fetch-xray.mjs`, factor the existing AdmZip extraction into a helper that can read named entries, extract both geo assets from the same verified release zip into `BIN_DIR`, write `<name>.sha256` sidecars, and include them in `verifyBinaries()` (non-override path). Keep `XRAY_BINARY_PATH` override behavior: skip bundled binary/asset verification as today, with the existing warning. Do not change `XRAY_VERSION` or asset selection.

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm vitest run scripts/fetch-xray.test.mjs scripts/release-config.test.mjs`
Expected: PASS — geo extraction, missing-asset failure, release config test.

- [ ] **Step 5: Commit**

```bash
git add scripts/xray-assets.mjs scripts/fetch-xray.mjs scripts/fetch-xray.test.mjs scripts/release-config.test.mjs apps/desktop/src-tauri/tauri.conf.json
git commit -m "feat: bundle xray geo assets"
```

### Task 5: Contracts, Settings tab, and RoutingEditor UI

**Files:**
- Modify: `packages/contracts/src/schemas.ts`
- Modify: `packages/contracts/src/schemas.test.ts`
- Modify: `apps/desktop/src/pages/Settings.tsx`
- Modify: `apps/desktop/src/pages/settings-page.test.tsx`
- Create: `apps/desktop/src/features/routing-editor/RoutingEditor.tsx`
- Create: `apps/desktop/src/features/routing-editor/routing-editor.test.tsx`

**Interfaces:**
- Consumes: ten Tauri commands from Task 2; payloads from Task 2. `invoke` helper from `apps/desktop/src/lib/invoke`.
- Produces: `RoutingProfileSchema`, `RoutingRuleSchema` Zod schemas; `RoutingEditor` default export; new `routing` settings tab.

- [ ] **Step 1: Write failing contract and UI tests**

In `packages/contracts/src/schemas.test.ts`, add:

```ts
it("parses a routing profile payload", () => {
  // RoutingProfileSchema.parse({ id, name, enabled, isActive, sort, createdAt, updatedAt })
});
it("parses routing rules with proxy/direct/block and rejects unknown outbound", () => {
  // RoutingRuleSchema.parse with domains/ips arrays; expect(() => parse("tunnel")).toThrow()
});
```

Update `apps/desktop/src/pages/settings-page.test.tsx` to expect five tabs and `grid-cols-5`.

In `apps/desktop/src/features/routing-editor/routing-editor.test.tsx`, mock `../../lib/invoke` with `vi.fn()` and write:

```tsx
it("loads profiles and rules and switches selection", async () => {
  // mock routing_profile_list -> two profiles, routing_rule_list -> one rule
  // click second profile, assert routing_rule_list called with its id
});
it("creates a rule with split textarea entries and outbound block", async () => {
  // fill remark, domains textarea "reg.ru\ngeosite:private", ips textarea "geoip:private, 10.0.0.0/8"
  // select outbound block, click save
  // assert routing_rule_create called with domains ["reg.ru","geosite:private"], ips ["geoip:private","10.0.0.0/8"], outbound "block"
});
it("toggles and reorders a rule", async () => {
  // toggle enabled -> routing_rule_update with enabled false
  // click down -> routing_rule_reorder with direction 1
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run packages/contracts/src/schemas.test.ts apps/desktop/src/pages/settings-page.test.tsx apps/desktop/src/features/routing-editor/routing-editor.test.tsx`
Expected: FAIL — schemas/component/tab missing.

- [ ] **Step 3: Implement contracts and Settings tab**

Add to `schemas.ts` (camelCase, matching Rust): `RoutingProfileSchema = z.object({ id: z.string(), name: z.string(), enabled: z.boolean(), isActive: z.boolean(), sort: z.number().int(), createdAt: timestamp, updatedAt: timestamp })`; `RoutingRuleSchema = z.object({ id, profileId, remark, enabled, outbound: z.enum(["proxy","direct","block"]), domains: z.array(z.string()), ips: z.array(z.string()), sort: z.number().int(), createdAt, updatedAt })`. Add `"routing"` to `SettingsTab`, tab array, render `<RoutingEditor/>` in its `<TabsContent>`, change `grid-cols-4` → `grid-cols-5`.

- [ ] **Step 4: Implement `RoutingEditor`**

Create `apps/desktop/src/features/routing-editor/RoutingEditor.tsx`. Use local React state only — no Zustand persistence. Layout: two columns; left profile list with name, enable `Switch`, active marker, `+` button; right rules table with enabled checkbox, remark, outbound, domain/IP preview, `+ Rule`; inline editor below with remark `Input`, outbound native `<select>` styled like `Input` (no Select primitive exists), enabled `Switch`, Domain and IP/CIDR `<textarea>`s, Save/Cancel, Delete, and up/down buttons. Split textarea values on newlines and commas, trim, drop empty entries. Load via `routing_profile_list` / `routing_rule_list`; mutations call the matching commands; refresh lists after each mutation. Show errors via the existing toast/sonner pattern. No profile drag-and-drop.

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm test`
Expected: PASS — full JS suite including new contract, settings-page, and routing-editor tests.

- [ ] **Step 6: Commit**

```bash
git add packages/contracts/src/schemas.ts packages/contracts/src/schemas.test.ts apps/desktop/src/pages/Settings.tsx apps/desktop/src/pages/settings-page.test.tsx apps/desktop/src/features/routing-editor
git commit -m "feat: add routing settings UI"
```

## Final Verification (after all tasks)

Run both suites:

```bash
pnpm test
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1
```

Expected: all green. Manual smoke with a real bundled xray binary remains operator-only (`pnpm installer:build` needs network + signing env); verify an active `block` rule actually blocks and `geosite:`/`geoip:` entries resolve with bundled assets. Update `AGENTS.md` and `docs/e2e-checklist.md` with the new Routing tab and asset requirements in the same commit as the final UI task if behavior docs are expected to stay current.

## Self-Review Notes

- Spec coverage: goals 1–8 map to Tasks 1–5; non-goals explicitly avoided (no port/protocol/network/process, no DNS rules, no import/export, no drag-drop, no Zustand persist, no proxy/tray change); anti-goals covered by Review Focus tests.
- Migration choice: `CREATE TABLE IF NOT EXISTS`, no version bump, pinned by `fresh_db_creates_routing_tables`.
- Asset path: `XRAY_LOCATION_ASSET` uses `resource_dir/binaries` because `bundle.resources` preserves the `binaries/` relative path.
- Apply point: connect/reconnect only; no attempt to mutate a running core.
- Type consistency: payload fields, repo signatures, config signature, Zod schemas, and command names use the identical spellings across tasks.
