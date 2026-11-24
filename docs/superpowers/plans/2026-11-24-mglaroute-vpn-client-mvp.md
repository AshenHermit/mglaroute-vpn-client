# mglaroute-vpn-client MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Windows desktop MVP: import subscriptions, maintain online-service filter profiles, auto-test/sort servers, auto-reconnect, and connect/disconnect via system proxy.

**Architecture:** Rust side owns SQLite, xray config/process, probing and Windows proxy; React calls a thin typed Tauri command surface and renders Zustand-driven UI. Shared TS/Zod contracts live in a pnpm workspace package; all mutations are serialized by one Rust token. Design is in `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md`.

**Tech Stack:** Tauri v2, Rust (rusqlite, reqwest, serde, tokio), Vite, React 18, TypeScript strict, Zustand 5, Tailwind 3, shadcn-style Radix wrappers, pnpm, Nx, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md` (approved 2026-11-24).

## Global Constraints

- Work from `D:\Users\hermit\vpn\mglaroute-vpn-client` (empty at start). No git repo exists — Task 1 runs `git init`.
- Stack fixed: tauri v2, vite, react, zustand, shadcn, tailwind, pnpm, nx.
- MVP platform: desktop Windows only. Linux/Android/iOS later. OS-specific code behind Rust traits.
- Anti-DPI is NOT functional in MVP. `AntiDpiService::start/stop/status` return `Unsupported`. Engine binaries + licence notices are bundled by the build script.
- xray and anti-DPI assets are bundled at build time, never downloaded on first run. Windows anti-DPI source: `bol-van/zapret-win-bundle` pinned commit `6eb463a6758fb48cd101bc55dfd057e6e9d98af1`, sha256 `5f828239310fed0da587bc09eaa22742615b2a85cf41f55ff28128e22231c6de`.
- Real delay: 2 GETs 100 ms apart through explicit loopback proxy, min positive ms, default ping URL `https://www.google.com/generate_204`, timeout `test_timeout_ms`.
- Service profile pass = **all** URLs return 2xx through same proxy. Empty URL list never passes. No direct-request fallback.
- Filter eligibility: active profile requires pass AND only passing servers eligible for auto-connect.
- Auto-reconnect after every probe cycle if current is not best; `reconnect_cooldown_seconds` gate; forced switch when current fails active filters.
- No auto-connect on app start. Previous system-proxy state persisted to disk and restored on start/crash.
- Global server pool across all enabled subscriptions; global `sort_index`; one connection.
- Mutating commands serialized by one Rust token; conflicting command returns `Busy`.
- Config generation and process launch consume the same immutable `ConfigSnapshot`.
- No subscription URLs, credentials or raw configs in logs.
- Commit after every task using Conventional Commits.

## Review Focus

1. **URL-safe base64 subscription body with `-`/`_` and no padding** must decode and parse — Task 4 test `decode_urlsafe_unpadded_base64`.
2. **One non-2xx URL inside an otherwise passing active service profile** must make the server ineligible — Task 9 test `profile_fails_when_any_url_non_2xx`.
3. **A malformed line among valid subscription links** must be skipped/counted, not abort the import — Task 4 test `bad_lines_skipped_valid_kept`.
4. **App restart after connect** must restore the previous proxy state from disk, not clear it blindly — Task 12 test `restore_previous_from_disk_on_start`.
5. **Overlapping probe/connect/refresh commands** must be rejected with `Busy` instead of spawning a second core — Task 10 test `concurrent_mutation_returns_busy`.

---

### Task 1: Workspace scaffold (pnpm + Nx + Tauri + React)

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml`, `nx.json`, `tsconfig.base.json`, `.gitignore`, `README.md`
- Create: `apps/desktop/package.json`, `apps/desktop/vite.config.ts`, `apps/desktop/index.html`, `apps/desktop/tsconfig.json`, `apps/desktop/src/main.tsx`, `apps/desktop/src/App.tsx`
- Create: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/src/lib.rs`
- Create: `packages/contracts/package.json`, `packages/ui/package.json`
- Test: `apps/desktop/src/App.test.tsx`, `apps/desktop/src-tauri/src/lib.rs` (`smoke` test)

**Interfaces:**
- Produces: `mglaroute_client_lib::run()` in `src-tauri/src/lib.rs`; Nx targets `dev`, `build`, `test`; workspace packages `@mglaroute/contracts`, `@mglaroute/ui`.

- [ ] **Step 1: Init workspace and git**

Run: `git init` then create root `package.json` with `packageManager: pnpm@9`, scripts `dev`, `build`, `test`, and `pnpm-workspace.yaml` listing `apps/*`, `packages/*`. Create `nx.json` with `namedInputs` default and `targetDefaults` for `build`/`test` (cache true). Create `tsconfig.base.json` with `strict: true`, `paths` for `@mglaroute/contracts` and `@mglaroute/ui`.

Expected: `pnpm install` succeeds; `git status` clean after initial commit.

- [ ] **Step 2: Scaffold React/Vite app**

Create `apps/desktop` with Vite React-TS config, Tailwind 3 config + `src/index.css` with `@tailwind` directives, minimal `App.tsx` rendering `<h1>mglaroute</h1>`. Add Vitest config and `App.test.tsx` asserting that text.

- [ ] **Step 3: Scaffold Tauri app**

Create `apps/desktop/src-tauri` with `Cargo.toml` deps `tauri`, `serde`, `serde_json`, `tokio` (features `rt-multi-thread`, `time`, `sync`, `macros`, `fs`), `rusqlite` (feature `bundled`), `reqwest` (features `json`, `rustls-tls`, no default TLS), `chrono` (feature `serde`), `sha2`, `futures`, `thiserror`, `windows-sys` (Windows-only, features `Win32_System_Registry`, `Win32_NetworkManagement_WinSock`, `Win32_System_SystemInformation`); dev-deps `tempfile`; `tauri.conf.json` with app identifier `com.mglaroute.client`, `beforeBuildCommand: pnpm build`, `frontendDist: ../dist`, and empty `externalBin` array (filled in Task 15). `lib.rs` exposes `pub fn run()` calling `tauri::Builder::default().run()`; `main.rs` calls it. Add `#[cfg(test)] mod smoke` test asserting `serde_json::json!({"ok": true}) == serde_json::json!({"ok": true})` placeholder is replaced by first real test in Task 2.

- [ ] **Step 4: Verify scaffold**

Run: `pnpm test && pnpm build && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: all pass; `git status` clean after.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: scaffold pnpm nx tauri react workspace"
```

---

### Task 2: SQLite schema and repositories

**Files:**
- Create: `apps/desktop/src-tauri/src/storage/mod.rs`, `storage/schema.sql`, `storage/models.rs`, `storage/repos.rs`
- Test: `storage/repos.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `rusqlite::Connection`.
- Produces: `migrate(conn: &Connection) -> Result<()>`; structs `Subscription`, `Profile`, `ServiceProfile`, `ProfileServiceResult`, `Settings`; functions `subscription_repo::{insert,get,list,update,remove}`, `profile_repo::{upsert_many,list_present,mark_absent,sort_all,set_result}`, `service_profile_repo::{insert,get,list,list_enabled,update,remove}`, `settings_repo::{get,update}`.

- [ ] **Step 1: Write failing tests**

In `storage/repos.rs` tests: `migrate_creates_all_tables` (open `:memory:`, call `migrate`, query `sqlite_master` for the five tables); `profile_upsert_preserves_latency` (insert profile with `latency_ms=42`, upsert same `server_id` with new `raw_config`, assert latency still 42); `profile_mark_absent` (upsert two, mark one absent, assert `list_present` returns one); `service_profile_unique_name` (second insert with same name returns error).

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage`
Expected: compile error — modules not found.

- [ ] **Step 3: Implement schema and models**

`schema.sql` with tables per spec (`subscriptions`, `profiles`, `service_profiles`, `profile_service_results`, `settings`), `PRAGMA user_version = 1`, UNIQUE `(subscription_id, server_id)` on profiles, UNIQUE on `service_profiles.name`. `migrate` executes `include_str!("schema.sql")` in a transaction. Define structs with `serde::{Serialize, Deserialize}` and exact field names from spec.

- [ ] **Step 4: Implement repositories**

Parameterized `rusqlite` statements only. `profile_repo::upsert_many` uses `ON CONFLICT(subscription_id, server_id) DO UPDATE` that does **not** overwrite `latency_ms`, `reachable`, `checked_at`. `sort_all` writes `sort_index=(i+1)*10` in the given order.

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage`
Expected: 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: add sqlite schema and repositories"
```

---

### Task 3: Shared contracts and typed invoke

**Files:**
- Create: `packages/contracts/src/index.ts`, `packages/contracts/src/schemas.ts`, `packages/contracts/package.json`
- Create: `apps/desktop/src/lib/invoke.ts`
- Test: `packages/contracts/src/schemas.test.ts`, `apps/desktop/src/lib/invoke.test.ts`

**Interfaces:**
- Consumes: Rust command/serde field names from Tasks 2, 5, 8, 13.
- Produces: Zod schemas `SubscriptionSchema`, `ProfileSchema`, `ServiceProfileSchema`, `SettingsSchema`, `ConnectionStateSchema`, `RefreshResultSchema`, `ProbeResultSchema`, `SettingsPatchSchema` + inferred types `Subscription`, `Profile`, `ServiceProfile`, `Settings`, `ConnectionState`, `RefreshResult`, `ProbeResult`, `SettingsPatch`; constants `EVENTS = { probeProgress, probeFinished, connectionState, subscriptionRefreshed }`; `invoke<T>(command, args)`. `ProbeResult` shape = `ProbeOutcome` from Task 10 with serde rename `finished_at`→`finishedAt`.

- [ ] **Step 1: Write failing tests**

`schemas.test.ts`: assert `SubscriptionSchema.parse({id:'s',name:'sub',url:'https://x',enabled:true,status:'active',auto_refetch_minutes:15,last_fetched_at:null,last_error:null,user_agent:null,request_headers:null,sort:0,created_at:'2026-01-01T00:00:00Z',updated_at:'2026-01-01T00:00:00Z'})` succeeds; assert `ServiceProfileSchema` rejects empty `name`. `invoke.test.ts`: mock `@tauri-apps/api/core.invoke`, assert `invoke` returns parsed value and throws on schema mismatch.

- [ ] **Step 2: Run tests to verify failure**

Run: `pnpm vitest run packages/contracts apps/desktop/src/lib`
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement contracts and wrapper**

Zod schemas mirror spec tables exactly; `ConnectionState` = `{ status: 'disconnected'|'connecting'|'connected'|'faulted', profileId: string|null, proxyPort: number|null }`. `invoke<T>` wraps Tauri `invoke`, parses with supplied schema, rethrows with command name in message (no URLs).

- [ ] **Step 4: Run tests to verify pass**

Run: `pnpm vitest run packages/contracts apps/desktop/src/lib`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add shared contracts and typed invoke"
```

---

### Task 4: Subscription decoder

**Files:**
- Create: `apps/desktop/src-tauri/src/subscription/mod.rs`, `subscription/decode.rs`
- Test: `subscription/decode.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing (pure).
- Produces: `ParsedProfile { link, remarks, protocol, address, port, raw_config: serde_json::Value }`; `DecodeOutcome { profiles: Vec<ParsedProfile>, bad_lines: usize }`; `decode_subscription(body: &str) -> DecodeOutcome`.

- [ ] **Step 1: Write failing tests**

Tests in `decode.rs`:
- `decode_plain_vless` — URI `vless://id@example.com:443?security=reality&sni=cdn.com&fp=chrome&pbk=abc&sid=de&type=raw#MyNode` yields protocol `vless`, address `example.com`, port 443, `raw_config.security=="reality"`, `raw_config.pbk=="abc"`, remarks `MyNode`.
- `decode_vmess_legacy_base64` — `vmess://<base64 of JSON with ps/add/port/id/aid/scy/net/host/path/tls/sni>` yields address/port/remarks from JSON.
- `decode_ss_sip002_and_legacy` — both forms yield protocol `ss` and non-empty `raw_config.method`.
- `decode_trojan_and_reality_params` — `trojan://pass@example.com:443?security=tls&sni=x&allowInsecure=1#T` yields `raw_config.sni=="x"`.
- `bad_lines_skipped_valid_kept` (Review Focus #3) — body with two valid links and `garbage line` → `profiles.len()==2`, `bad_lines==1`.
- `decode_urlsafe_unpadded_base64` (Review Focus #1) — whole body encoded as URL-safe base64 with `-`/`_` and padding stripped, containing two links → both parsed.
- `dedup_exact_links` — duplicate line counted once.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml subscription::decode`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement decoder**

Cascade: try whole-body base64 (normalize `-`→`+`, `_`→`/`, strip whitespace, add `=` padding to multiple of 4); if decoded contains a URI marker (`vmess://`, `vless://`, `trojan://`, `ss://`) use decoded text else original. Split lines, `trim`, skip empty, `HashSet` dedup preserving order. Dispatch by prefix; parse query params into `raw_config` with whitelist `security,sni,fp,pbk,sid,spx,flow,encryption,type,path,host,alpn,serviceName,headerType,mode,packetEncoding,allowInsecure,skip-cert-verify`. Unrecognized prefixes increment `bad_lines`.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml subscription::decode`
Expected: 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add cascading subscription decoder"
```

---

### Task 5: Subscription fetch + import orchestration

**Files:**
- Create: `apps/desktop/src-tauri/src/subscription/fetch.rs`, `subscription/import.rs`
- Test: `subscription/import.rs` `#[cfg(test)] mod tests`, `subscription/fetch.rs` tests

**Interfaces:**
- Consumes: `decode_subscription`, `subscription_repo`, `profile_repo`.
- Produces: `fetch_subscription(client: &reqwest::Client, url: &str, user_agent: Option<&str>) -> Result<String>`; `import_subscription_text(conn: &mut Connection, sub_id: &str, body: &str) -> Result<ImportSummary>` where `ImportSummary { added, updated, absent, bad_lines }`.

- [ ] **Step 1: Write failing tests**

`import.rs`: `upsert_existing_keeps_delay` (pre-insert profile with latency 42, import same link with changed raw_config, assert latency preserved); `missing_links_marked_absent` (import two then import one, assert other `present=false`); `bad_lines_counted` (import body with garbage → `bad_lines==1`). `fetch.rs`: `fetch_uses_no_system_proxy` — spin a local `TcpListener` returning a fixed body, assert function returns that body while environment `HTTPS_PROXY` is set to a dead address; `fetch_http_error_returns_err` — local server returns 404.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml subscription`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement fetch and import**

`fetch_subscription`: `reqwest::Client` with `.no_proxy()`, 10 s timeout, optional User-Agent, `.error_for_status()`. `import_subscription_text`: decode then `profile_repo::upsert_many`; `server_id = sha256(link)` hex; compute absent by comparing existing `server_id`s; set `present=false` for missing; on success set `subscription.status='active'`, `last_fetched_at=now`, `last_error=NULL`; caller sets error state.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml subscription`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add subscription fetch and import"
```

---

### Task 6: Service profiles and settings storage

**Files:**
- Modify: `apps/desktop/src-tauri/src/storage/repos.rs`
- Create: `apps/desktop/src-tauri/src/service_profiles.rs`
- Test: `apps/desktop/src-tauri/src/service_profiles.rs` tests

**Interfaces:**
- Consumes: `service_profile_repo`, `settings_repo`.
- Produces: `seed_defaults(conn: &mut Connection) -> Result<()>` inserting YouTube/Discord/Google only when a row with that `name` is absent; `ensure_defaults_on_empty(conn)`; `validate_test_urls(&[String]) -> bool` (non-empty, all `https://` and parseable).

- [ ] **Step 1: Write failing tests**

`seed_defaults_idempotent` (call twice → 3 rows); `seed_does_not_overwrite_existing` (pre-insert YouTube with custom URLs → seed keeps custom); `validate_test_urls` rejects empty list, `http://example.com`, and `not a url`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml service_profiles`
Expected: FAIL.

- [ ] **Step 3: Implement**

Mirror `../mglaroute-vpn-server/backend/src/online-services/online-services.service.ts`: seeds `{YouTube, ['https://www.youtube.com']}`, `{Discord, ['https://discord.com']}`, `{Google, ['https://www.google.com']}` with `is_default=true`. `is_default` is marker only — no delete protection, no auto-enable.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml service_profiles`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: seed and validate service profiles"
```

---

### Task 7: Xray config builder and snapshot

**Files:**
- Create: `apps/desktop/src-tauri/src/core/mod.rs`, `core/snapshot.rs`, `core/config.rs`
- Test: `core/config.rs` tests

**Interfaces:**
- Consumes: `Profile`, `Settings`.
- Produces: `ConfigSnapshot { subscription_id: String, profile: Profile, proxy_port: u16, config_path: PathBuf, anti_dpi: Option<()> }`; `snapshot_for(profile: &Profile, port: u16, dir: &Path) -> ConfigSnapshot`; `build_speedtest_config(profiles: &[Profile], ports: &[u16]) -> Result<serde_json::Value>`; `build_connection_config(snapshot: &ConfigSnapshot, exclusions: &[String]) -> Result<serde_json::Value>`.

- [ ] **Step 1: Write failing tests**

`speedtest_config_shape` — 2 profiles/2 ports → 2 `mixed` inbounds with `listen:"127.0.0.1"`, `protocol:"mixed"`, `noauth:true`, 2 outbounds, 2 routing rules mapping inbound tag→outbound tag, `log.loglevel=="warning"`. `connection_config_shape` — one inbound on snapshot port, one outbound from profile, one `freedom` outbound named `direct`, routing bypass rule for `example.com` from exclusions. `snapshot_is_immutable_value` — two snapshots from same profile+port+dir serialize equal.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::config`
Expected: FAIL.

- [ ] **Step 3: Implement**

Use `serde_json::json!` assembly from v2rayN-like template: `log`, `inbounds[]`, `outbounds[]`, `routing.rules[]`. `build_connection_config` tags: inbound `proxy-in`, outbound `proxy-out`, direct `direct`. Exclusions become `rule: "field":"domain","outboundTag":"direct"`.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::config`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add xray config builder and snapshot"
```

---

### Task 8: Xray supervisor + port readiness

**Files:**
- Create: `apps/desktop/src-tauri/src/core/supervisor.rs`
- Test: `core/supervisor.rs` tests

**Interfaces:**
- Consumes: `ConfigSnapshot`, bundled binary path from `core::binary_path`.
- Produces: `binary_path(override: Option<&str>) -> PathBuf` honoring `XRAY_BINARY_PATH` then `../binaries/xray.exe`; `build_command(snapshot: &ConfigSnapshot) -> Result<std::process::Command>`; `wait_for_port(port: u16, timeout: Duration) -> Result<()>`; `ChildGuard` with `stop(&mut self) -> Result<()>`.

- [ ] **Step 1: Write failing tests**

`command_matches_snapshot` — assert program ends with `xray.exe` (on Windows), args are exactly `["run","-c",<snapshot.config_path>]`. `wait_for_port_success` — bind `TcpListener` on ephemeral port, assert `wait_for_port(port, 1s)` ok. `wait_for_port_timeout` — unused port, assert err within 1 s. `binary_path_override` — set `XRAY_BINARY_PATH=/tmp/custom`, assert returned path equals it.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::supervisor`
Expected: FAIL.

- [ ] **Step 3: Implement**

`wait_for_port`: TCP connect loop with 50 ms sleep until deadline. `ChildGuard::stop`: `kill()` then `wait()`; on Windows wrap in Job Object only if easy — otherwise note as follow-up (do not block MVP). Redirect stdout/stderr to null in MVP (no secrets in logs).

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::supervisor`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add xray process supervisor"
```

---

### Task 9: Real-delay and service checks

**Files:**
- Create: `apps/desktop/src-tauri/src/probe/mod.rs`, `probe/delay.rs`, `probe/service_check.rs`
- Test: `probe/delay.rs` tests, `probe/service_check.rs` tests

**Interfaces:**
- Consumes: `reqwest`.
- Produces: `measure_delay(proxy_url: &str, ping_url: &str, timeout: Duration) -> Option<u64>`; `check_service_profile(urls: &[String], proxy_url: &str, timeout: Duration) -> bool`.

- [ ] **Step 1: Write failing tests**

Build a local mock HTTP proxy in tests (`TcpListener` that reads the request line and replies `HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n`): `delay_uses_min_positive_ms` (two requests → `Some(n)` with `n>0`); `delay_timeout_returns_none` (listener accepts but never replies, timeout 200 ms → `None`); `profile_fails_when_any_url_non_2xx` (Review Focus #2) — 2 URLs, mock returns 204 for first and 302 for second → `false`; `service_check_uses_explicit_proxy` (set `HTTPS_PROXY` to dead address, local mock still used → `true`); `empty_urls_never_pass` → `false`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe`
Expected: FAIL.

- [ ] **Step 3: Implement**

`reqwest::Client::builder().proxy(reqwest::Proxy::all(proxy_url)?).timeout(timeout).build()`. For delay do 2 GETs 100 ms apart, collect positive `elapsed.as_millis()`, return min or `None`. For service check `Promise.all`-equivalent via `futures::future::join_all`, `validateStatus` = 2xx only, `false` on any error/non-2xx, `false` for empty list.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add real delay and service checks"
```

---

### Task 10: Probe cycle, scheduler gates and concurrency token

**Files:**
- Create: `apps/desktop/src-tauri/src/probe/cycle.rs`, `probe/scheduler.rs`, `state.rs`
- Test: `probe/scheduler.rs` tests, `state.rs` tests

**Interfaces:**
- Consumes: `build_speedtest_config`, `XraySupervisor`, `measure_delay`, `check_service_profile`, repos.
- Produces: `should_refetch(sub: &Subscription, now: DateTime<Utc>, enabled: bool) -> bool`; `run_probe_cycle(conn: &mut Connection, settings: &Settings) -> Result<ProbeOutcome>`; `MutationGuard` (tokio semaphore with `try_acquire`, `Busy` error).

- [ ] **Step 1: Write failing tests**

`refetch_gate_requires_both_flags` — global off → false; per-sub interval 0 → false; elapsed < interval → false; elapsed >= interval → true. `concurrent_mutation_returns_busy` (Review Focus #5) — acquire guard twice on tokio runtime, second returns `Busy` not wait. `probe_cycle_sorts_failed_last` — use injected test double for delay/check (trait `ProbeTransport`) returning delays `[None, Some(20), Some(10)]`, assert resulting `sort_index` order is 10, 20, failed.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe::scheduler state`
Expected: FAIL.

- [ ] **Step 3: Implement**

`MutationGuard` uses `tokio::sync::Semaphore(1)` + `try_acquire_owned`; `Busy` error type. `run_probe_cycle`: profiles from `list_present`, batches of 20, one temp config per batch via `build_speedtest_config`, spawn core, measure delay + each active service profile, kill core in `finally`, write results and `sort_all`, emit progress events. Define `trait ProbeTransport` for testability; production impl uses xray supervisor.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe::scheduler state probe::cycle`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add probe cycle, scheduler gates and mutation guard"
```

---

### Task 11: Auto-reconnect decision

**Files:**
- Create: `apps/desktop/src-tauri/src/reconnect.rs`
- Test: `reconnect.rs` tests

**Interfaces:**
- Consumes: `Profile`, `ProfileServiceResult`, `Settings`, `DateTime<Utc>`.
- Produces: `ProbeOutcome { tested, sorted, finished_at }` serialized as `ProbeResult` (`finishedAt` in TS); `enum ReconnectDecision { None, Switch { profile_id: String }, Disconnect }`; `decide_reconnect(current_id: Option<&str>, candidates: &[(String, u64, bool)], settings: &Settings, now: DateTime<Utc>, last_switch: Option<DateTime<Utc>>) -> ReconnectDecision` where tuple = `(profile_id, latency_ms, passes_all_active_profiles)`.

- [ ] **Step 1: Write failing tests**

`no_better_candidate_none` — current is best → `None`. `better_candidate_switches_after_cooldown` — lower delay candidate, cooldown elapsed → `Switch`. `cooldown_blocks_switch` → `None`. `current_fails_filters_forces_switch_ignoring_cooldown` — current `passes=false`, candidate passes → `Switch` despite cooldown. `no_candidate_passing_disconnects` → `Disconnect`. `no_active_profiles_uses_delay_only` — all `passes=true` vacuous → min delay wins. `no_measurement_no_switch` — empty candidates → `None`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reconnect`
Expected: FAIL.

- [ ] **Step 3: Implement**

Pure function. Only candidates with `passes=true` considered. Best = min latency. If current is not in passing set → `Disconnect` when no passing candidate exists, else `Switch` to best ignoring cooldown. Else switch to best only if `best != current` and cooldown elapsed. Empty candidates → `None`.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reconnect`
Expected: 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add auto-reconnect decision logic"
```

---

### Task 12: Windows system proxy and persisted restore

**Files:**
- Create: `apps/desktop/src-tauri/src/platform/mod.rs`, `platform/proxy.rs`, `platform/proxy_windows.rs`
- Test: `platform/proxy.rs` tests

**Interfaces:**
- Consumes: `ConfigSnapshot`.
- Produces: `enum PreviousProxy { Direct, Pac(String), Manual { server: String, override_list: String } }`; `trait ProxyBackend { fn apply(&self, port: u16, exclusions: &[String]) -> Result<()>; fn restore(&self, previous: &PreviousProxy) -> Result<()>; }`; `WindowsProxy`; `InMemoryProxy` (test double); `persist_previous(path: &Path, p: &PreviousProxy) -> Result<()>`; `load_previous(path: &Path) -> Result<PreviousProxy>`; `proxy_string(port: u16) -> String`; `override_string(exclusions: &[String]) -> String` prepending `<local>`.

- [ ] **Step 1: Write failing tests**

`proxy_string_shape` → `"127.0.0.1:10808"`. `override_prepends_local` → `["example.com"]` → `"<local>;example.com"`. `restore_previous_from_disk_on_start` (Review Focus #4) — persist `Manual{...}`, create fresh backend, load and restore, assert backend state equals previous. `in_memory_apply_then_restore` — apply port/exclusions then restore → previous state back.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml platform::proxy`
Expected: FAIL.

- [ ] **Step 3: Implement**

Windows impl: read/write registry `Software\Microsoft\Windows\CurrentVersion\Internet Settings` (`ProxyEnable`, `ProxyServer`, `ProxyOverride`), call `InternetSetOption` with `INTERNET_OPTION_SETTINGS_CHANGED` (39) and `INTERNET_OPTION_REFRESH` (37) via `windows-sys` P/Invoke; also iterate RAS entries as v2rayN does if feasible, otherwise note as follow-up. Persist JSON to app-data `proxy_previous.json` on first apply; never overwrite it while connected. On startup call `load_previous` + `restore` if present, then delete file. `InMemoryProxy` implements same trait for tests.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml platform::proxy`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add windows proxy with disk-backed restore"
```

---

### Task 13: Tauri commands, events and anti-DPI stubs

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/mod.rs`, `commands/subscription.rs`, `commands/service_profile.rs`, `commands/probe.rs`, `commands/connection.rs`, `commands/settings.rs`
- Create: `apps/desktop/src-tauri/src/anti_dpi.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `apps/desktop/src-tauri/src/commands/mod.rs` tests

**Interfaces:**
- Consumes: all Tasks 5–12.
- Produces: commands `subscription_add`, `subscription_list`, `subscription_update`, `subscription_remove`, `subscription_refresh`, `service_profile_list/create/update/remove`, `settings_get/update`, `probe_run`, `connection_status`, `connection_connect(profile_id: Option<String>)`, `connection_disconnect`, `anti_dpi_status`. Events emitted: `probe-progress`, `probe-finished`, `connection-state`, `subscription-refreshed`.

- [ ] **Step 1: Write failing tests**

`connect_remote_uses_best_eligible` — seed 3 profiles with delays 40/10/25 and one failing active service profile, call decision helper through command test harness, assert chosen profile has delay 10 among passing. `anti_dpi_stub_unsupported` — assert `anti_dpi_status()` returns `Unsupported`. `command_names_registered` — build `tauri::Builder` in test and assert invoke handler list contains all command names (or compile-time assertion via macro list).

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands`
Expected: FAIL.

- [ ] **Step 3: Implement commands**

Every mutating command acquires `MutationGuard` first and returns `Busy` on conflict. `subscription_refresh` calls `fetch_subscription` then `import_subscription_text`, emits `subscription-refreshed`. `probe_run` calls `run_probe_cycle` then `decide_reconnect` and applies switch/disconnect. `connection_connect(None)` selects best eligible profile from global pool; `Some(id)` connects that profile if present. `anti_dpi.rs` exposes `start/stop/status` returning `Unsupported` in MVP (`AntiDpiStatusSchema` in contracts = `{status:'unsupported'}`). In `lib.rs::run()` start two `tokio::time::interval` loops with a 1-minute tick: refetch loop calls `should_refetch` for every enabled subscription and runs `subscription_refresh`; auto-test loop checks `settings.auto_test_enabled` and calls `probe_run`. On startup, before any loop, call `load_previous` + `restore` from Task 12; do not auto-connect. Serialize command payload names with `#[serde(rename_all = "camelCase")]` so `profile_id`→`profileId` and `finished_at`→`finishedAt` match TS contracts.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: wire tauri commands and events"
```

---

### Task 14: React UI (dashboard, subscriptions, service profiles, servers, settings)

**Files:**
- Create: `apps/desktop/src/stores/app.store.ts`, `stores/connection.store.ts`
- Create: `apps/desktop/src/features/connection-panel/*`, `features/subscription-list/*`, `features/server-table/*`, `features/service-profile-editor/*`, `features/settings-form/*`
- Create: `apps/desktop/src/pages/{Dashboard,Subscriptions,ServiceProfiles,Servers,Settings}.tsx`
- Create: `apps/desktop/src/components/ui/*` (shadcn primitives: button, input, switch, table, dialog, card)
- Modify: `apps/desktop/src/App.tsx`
- Test: one Vitest + Testing Library test per feature

**Interfaces:**
- Consumes: `@mglaroute/contracts` schemas, `invoke`.
- Produces: `useAppStore`, `useConnectionStore`; pages wired to commands; event listeners for `probe-progress`, `connection-state`, `subscription-refreshed`.

- [ ] **Step 1: Write failing tests**

`connection-panel.test.tsx` — clicking Connect calls `invoke('connection_connect', {profileId: null})` and renders state from `connection-state` event. `subscription-list.test.tsx` — Add form posts URL, list shows `status='error'` badge with red style. `service-profile-editor.test.tsx` — adding a profile with empty URL list shows validation error and does not invoke. `server-table.test.tsx` — sorted list shows delay ascending, failed rows last and greyed. `settings-form.test.tsx` — toggling auto-refetch persists via `settings_update`.

- [ ] **Step 2: Run tests to verify failure**

Run: `pnpm vitest run apps/desktop/src`
Expected: FAIL.

- [ ] **Step 3: Implement UI**

Zustand stores hold UI/selection only; durable data comes from commands, never `persist` for subscription URLs or raw configs. Tailwind + shadcn-style Radix wrappers. Keep settings form fields exactly: `auto_refetch_enabled`, `refetch_interval_minutes`, `auto_test_enabled`, `test_interval_minutes`, `test_timeout_ms`, `auto_reconnect_enabled`, `reconnect_cooldown_seconds`, `ping_url`, `local_proxy_port`, `proxy_exclusions`, `system_proxy_enabled`.

- [ ] **Step 4: Run tests to verify pass**

Run: `pnpm vitest run apps/desktop/src && pnpm build`
Expected: all PASS, production build succeeds.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add desktop UI"
```

---

### Task 15: Build-time binary bundling and licences

**Files:**
- Create: `scripts/fetch-xray.mjs`, `scripts/stage-antiddpi.mjs`, `scripts/xray-assets.mjs`, `scripts/http-get.mjs`, `scripts/verify-sha256.mjs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json` (`externalBin`)
- Create: `third_party/THIRD_PARTY_NOTICES.md`, `third_party/licenses/*`
- Test: `scripts/verify-sha256.test.mjs`, `scripts/xray-assets.test.mjs`

**Interfaces:**
- Consumes: `backend/scripts/download-xray.mjs` pattern from server repo.
- Produces: `pnpm binaries:fetch` populating `apps/desktop/src-tauri/binaries/` with `xray.exe` (Windows x64) and anti-DPI bundle assets; `pnpm binaries:verify` checking sha256 and `xray -version` exit 0.

- [ ] **Step 1: Write failing tests**

`select_asset_maps_win32_x64` — assert mapping to `Xray-win7-64.zip` first then `Xray-windows-64.zip`. `verify_sha256_rejects_mismatch` — temp file + wrong hash → throws. `verify_sha256_accepts_match` — correct hash → ok.

- [ ] **Step 2: Run tests to verify failure**

Run: `pnpm vitest run scripts`
Expected: FAIL.

- [ ] **Step 3: Implement scripts**

Port server logic: GitHub releases latest API with 403/429 → latest redirect tag fallback; download `.zip` and `.dgst` sequentially; sha256 verify; extract only `/xray(.exe)?$/`; chmod 755; run `xray -version`. `stage-antiddpi.mjs` downloads pinned `zapret-win-bundle` commit and verifies sha256, staging `winws2.exe`, `cygwin1.dll`, `WinDivert.dll`, `WinDivert64.sys`, `lua/*.lua`. Copy all third-party licences. Set Tauri `externalBin` for xray; anti-DPI assets bundled as resources (not invoked in MVP).

- [ ] **Step 4: Run tests to verify pass**

Run: `pnpm vitest run scripts && pnpm binaries:verify`
Expected: all PASS; `xray -version` exits 0.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: bundle xray and anti-dpi assets at build time"
```

---

### Task 16: End-to-end verification and docs

**Files:**
- Modify: `README.md`, `AGENTS.md`
- Create: `docs/e2e-checklist.md`

**Interfaces:**
- Consumes: all previous tasks.
- Produces: documented manual E2E checklist and known limitations.

- [ ] **Step 1: Write checklist**

`docs/e2e-checklist.md` steps: launch with no proxy change → no auto-connect; import a real subscription URL → profiles appear; add active service profile with 2 URLs → run probe → only passing servers sorted first; connect → registry proxy set and traffic flows; kill xray process → state `Faulted`; disconnect → previous proxy restored; restart app → proxy restored from disk, still disconnected; auto-refetch tick updates `last_fetched_at`; cooldown blocks rapid reconnect.

- [ ] **Step 2: Run full verification**

Run: `pnpm test && pnpm build && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml && pnpm binaries:verify`
Expected: all PASS.

- [ ] **Step 3: Update docs**

Record exact commands and known limitations (no TUN, no anti-DPI, no Clash YAML import, no JSON feed import, RAS iteration if skipped).

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: add e2e checklist and known limitations"
```

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-11-24-mglaroute-vpn-client-mvp.md`. Please review the plan. Which execution approach would you prefer?

- **Subagent-driven** — fresh subagent implements each task and a fresh reviewer checks it before the next starts, then a whole-branch review at the end. Most thorough; costs a fresh context per task and review.
- **Native** — I implement every task myself in this session, then one reviewer checks the whole branch. Cheapest and fastest; works well because the plan carries the exact interfaces.

For this plan I recommend **subagent-driven**: tasks are highly interface-coupled (Rust command names and SQLite fields must match exactly), and a shipped mistake in proxy restore or probe eligibility would break real traffic. Does the plan capture what you want, and which approach should we use?
