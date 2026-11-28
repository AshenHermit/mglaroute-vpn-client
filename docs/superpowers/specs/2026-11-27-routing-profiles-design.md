# Routing profiles (v2rayN-style) — design

Status: accepted  
Date: 2026-11-27

## Intent

The desktop client currently has a single hard-coded routing shape: private-IP
bypass, domain exclusions from `settings.proxy_exclusions`, then a catch-all
proxy rule. There is no way to send selected domains/IPs direct or block them
through a reusable, named set of rules.

Add a **Routing** area to Settings where the operator can maintain named
routing profiles. Each profile contains ordered rules. A rule lists domains
and/or IP/CIDR entries (including `geosite:` / `geoip:` prefixes) and selects an
outbound: `proxy`, `direct`, or `block`. Profiles and individual rules can be
enabled or disabled.

This mirrors the v2rayN rule list/rule editor the operator attached, but uses
the client's existing SQLite + Tauri command patterns rather than v2rayN's
single JSON `RuleSet` blob.

## Goals

- Create, rename, delete, enable/disable, and activate routing profiles.
- Exactly one profile can be **active** at a time; activating another clears
  the previous active flag.
- Create, edit, delete, enable/disable, and reorder rules inside a profile.
- Rule fields: `remark`, `enabled`, `outbound` (`proxy` | `direct` | `block`),
  `domains: string[]`, `ips: string[]`, `sort`.
- Domains/IPs accept plain values and `geosite:` / `geoip:` prefixes.
- Bundle `geoip.dat` and `geosite.dat` so prefixed entries work out of the box.
- Only the active profile contributes user rules to the live connection config.
- Probe config stays untouched: every probe request must still go through the
  proxy.

## Non-goals

- No port, protocol, network/inboundTag, or process matching (all explicitly
  declined).
- No DNS-specific rules.
- No import/export of v2rayN routing JSON, Clash YAML, or sing-box rule sets.
- No drag-and-drop reordering in v1; up/down controls are enough.
- No new Zustand persisted state; rules and profiles live in SQLite.
- No change to system-proxy connect/disconnect or tray lifecycle behavior.

## Anti-goals (failure even if it compiles)

- Breaking probes: a temporary probe core must never consult routing profiles,
  private bypass, or exclusions.
- Silently ignoring the active profile's `block` rules because the xray config
  has no `blackhole` outbound.
- Shipping `geosite:`/`geoip:` support that fails at runtime because the asset
  files are not actually reachable by xray.
- Letting a disabled profile or disabled rule affect a live connection.

## Constraints

- Stack fixed: Tauri v2 + Rust/`rusqlite`, React 18 + TypeScript strict,
  shadcn-style local Radix wrappers + Tailwind 3, Zustand 5 with `persist` only
  for non-secret UI prefs. Durable data in SQLite.
- Existing schema is `PRAGMA user_version = 2`; `storage/mod.rs::migrate` only
  has v1 → v2 (`test_concurrency`). New tables can be added with
  `CREATE TABLE IF NOT EXISTS` in `schema.sql` without a version bump; if a
  migration is required instead, bump the version deliberately.
- `ConfigSnapshot` is documented as the immutable input for one xray process.
  Config generation must stay deterministic for identical inputs.
- `lib.rs::command_list!` registers every Tauri command, and a test asserts the
  command-name surface; new commands must be added there.
- Rust emits `camelCase` payloads; Zod contracts in
  `packages/contracts/src/schemas.ts` must match.
- Build fetch script currently extracts only the xray binary from the release
  zip; asset files must be added to the same verified fetch path.
- Caveman-mode terse output; exact identifiers preserved.

## Approaches considered

### A — v2rayN-style single JSON `RuleSet` column

Store one `rules TEXT` JSON blob per profile, like v2rayN `RoutingItem.RuleSet`
and `RulesItem[]`. Editing any rule rewrites the whole blob.

Rejected: loses per-rule point updates, makes enable/disable and reorder a
read-modify-write of a blob, duplicates validation in TS and Rust, and does not
fit the `service_profiles` / normalized-table pattern already used here.

### B — Two normalized SQLite tables (chosen) ✅

`routing_profiles` holds profile metadata; `routing_rules` holds one row per
rule with `profile_id` FK. Per-rule toggles and reorder are point updates;
validation lives in Rust. This matches the client's existing repositories and
command style.

Trade-off: slightly more SQL and a repo layer, but the UI's master/detail model
maps directly to the tables and the active-profile query is one indexed read.

### C — Reuse `settings.proxy_exclusions` only

Extend the existing exclusion list with an outbound selector and call it done.

Rejected: no named profiles, no active-profile concept, no per-rule enable, and
no ordered first-match semantics. Does not meet the request.

## Decisions (confirmed with operator)

1. **One active profile.** Multiple profiles may be `enabled`, but exactly one
   may be `is_active = 1`. Activating a profile clears `is_active` on all other
   profiles in the same transaction. Disabling the active profile clears its
   active flag and leaves no active profile (fallback = current built-in
   bypass + exclusions + catch-all proxy).
2. **Rule fields limited** to `domains`, `ips`, `outbound`, `remark`,
   `enabled`, `sort`. `geosite:` and `geoip:` prefixes are accepted as entry
   values. Port/protocol/network/inboundTag/process are out of scope.
3. **Bundle geo assets.** `geosite.dat` and `geoip.dat` ship in the installer;
   accept the size increase.
4. **UI layout B:** two-pane master/detail inside Settings → Routing. Left =
   profile list with enable + active indicators and `+`. Right = rules table
   for the selected profile with `+ Rule`; inline editor below the table with
   outbound selector (`proxy`/`direct`/`block`), enabled toggle, and separate
   Domain and IP/CIDR textareas. Operator picked this mockup in the browser.
5. **Storage:** two normalized tables, not a JSON `RuleSet` blob.

## Call made by designer (builder may implement equivalently)

- `enabled` means "eligible to be used/activated"; `is_active` means "the one
  currently feeding xray config". Only an enabled profile can be activated.
  A disabled profile's rules never reach config even if its active flag was
  stale.
- Rule order is `sort ASC, id ASC` for deterministic first-match. Within one
  user rule, domain entries and IP entries are emitted as **separate** xray
  rules (domains first, then IPs), following v2rayN's split rather than relying
  on ambiguous mixed `domain`+`ip` matching semantics.
- First matching generated rule wins. Each rule maps `proxy` → `proxy-out`,
  `direct` → `direct`, `block` → a new `blackhole` outbound tagged `block`.
- Built-in rules keep priority: private-IP bypass first, then
  `settings.proxy_exclusions`, then active user rules, then catch-all
  `proxy-out`. Private bypass cannot be overridden by a user `proxy` rule.
- `block` outbound shape: `{ "tag": "block", "protocol": "blackhole",
  "settings": {} }`.

## Data shape

`routing_profiles`:

| column | type | notes |
| --- | --- | --- |
| `id` | TEXT PK | `rtp-<sha256>` like `svc-` IDs |
| `name` | TEXT NOT NULL UNIQUE | non-empty, Rust-validated |
| `enabled` | INTEGER NOT NULL DEFAULT 1 | boolean |
| `is_active` | INTEGER NOT NULL DEFAULT 0 | at most one row = 1 |
| `sort` | INTEGER NOT NULL DEFAULT 0 | list order |
| `created_at`, `updated_at` | TEXT | same convention as other tables |

`routing_rules`:

| column | type | notes |
| --- | --- | --- |
| `id` | TEXT PK | `rtr-<sha256>` |
| `profile_id` | TEXT NOT NULL REFERENCES routing_profiles(id) ON DELETE CASCADE | |
| `remark` | TEXT NOT NULL DEFAULT '' | display label |
| `enabled` | INTEGER NOT NULL DEFAULT 1 | boolean |
| `outbound` | TEXT NOT NULL CHECK IN ('proxy','direct','block') | |
| `domains` | TEXT NOT NULL DEFAULT '[]' | JSON `string[]` in DB |
| `ips` | TEXT NOT NULL DEFAULT '[]' | JSON `string[]` in DB |
| `sort` | INTEGER NOT NULL DEFAULT 0 | order within profile |
| `created_at`, `updated_at` | TEXT | |

Command payloads (camelCase): `RoutingProfilePayload { id, name, enabled,
isActive, sort, createdAt, updatedAt }`; `RoutingRulePayload { id, profileId,
remark, enabled, outbound, domains, ips, sort, createdAt, updatedAt }`. Add
matching Zod schemas to `packages/contracts/src/schemas.ts`.

## Behavior

### Config generation

`build_connection_config` takes a third argument: resolved active rules
(`&[RoutingRule]`). It emits, in order:

1. existing `XRAY_BYPASS_IPS` → `direct`
2. existing `settings.proxy_exclusions` domain rules → `direct`
3. for each enabled rule of the active profile, in `sort` order:
   - one `field` rule with `domain: [...domains]` when domains non-empty,
     `outboundTag` from rule
   - one `field` rule with `ip: [...ips]` when IPs non-empty,
     `outboundTag` from rule
4. catch-all `inboundTag: ["proxy-in"]` → `proxy-out`

Outbounds: existing `proxy-out`, existing `direct`, plus `block` when at least
one active rule uses it (or always — builder's call). `build_probe_config`
keeps its current single-rule shape and must not read routing state.

Entries like `geosite:private` go into the `domain` array; `geoip:private` goes
into the `ip` array. No rewriting in Rust beyond trimming empty entries.

Validation (Rust side, `InvalidInput`): non-empty profile name; unique name;
`outbound` in the allowed set; no blank-only entries; domains/IPs arrays parse
as JSON strings. Builder may add stricter shape checks (CIDR parse, prefix
syntax) as long as prefixed entries remain valid.

### Activation

`routing_profile_activate(id)` runs in one DB transaction: reject if profile
missing or disabled; set all rows `is_active = 0`; set target `is_active = 1`.
`routing_profile_update` with `enabled: false` also clears `is_active` for that
row. Removing the active profile leaves no active profile.

### Applying changes to a live connection

Profile/rule edits are durable immediately but do not silently swap the
running core. The operator connects (or reconnects) to apply the new active
rules, same as changing `proxy_exclusions` today. This keeps connect as the
single apply point and avoids mutating a running snapshot.

## UI

Add `routing` to `SettingsTab` and the tab array; change `TabsList` from
`grid-cols-4` to `grid-cols-5`. New feature folder
`src/features/routing-editor/` with a `RoutingEditor` component.

Left pane: profile list rows show name, enable switch, active marker, and a
small `+` button. Right pane: table of rules for the selected profile with
checkbox (enabled), remark, outbound, domain/IP preview, and `+ Rule`; inline
editor below supports create/edit, outbound radio/select, enabled switch,
Domain textarea (one entry per line, split by comma/newline), IP/CIDR textarea,
Save/Cancel, Delete, and up/down reorder. Local component state loads via new
Tauri commands; no new persisted Zustand slice.

## Build and assets

- `scripts/xray-assets.mjs`: keep current version mapping; add expected asset
  names `geoip.dat`, `geosite.dat`.
- `scripts/fetch-xray.mjs`: after extracting `xray(.exe)`, extract both `.dat`
  files from the same release zip into the staged binaries dir and write
  `.sha256` sidecars, using the same verification path as the binary.
- `tauri.conf.json` `bundle.resources`: add the staged dat files (e.g.
  `binaries/geoip.dat`, `binaries/geosite.dat`).
- Confirm at implementation time how xray locates those resources from the
  installed bundle (root config asset fields vs resource-dir/env). Do not
  assume default exe-dir lookup if Tauri installs resources elsewhere.

## What's left to the builder

- Exact repo module (`storage/routing_repo.rs` or extension of `repos.rs`),
  command names, and `command_list!` registration.
- Exact migration choice: `CREATE TABLE IF NOT EXISTS` in `schema.sql` (no
  version bump) vs a v2 → v3 migration. Verify with a fresh-DB test.
- Whether `build_connection_config` receives rules as a third arg (as above)
  or they are added to `ConfigSnapshot`; preserve determinism and probe
  isolation either way.
- Whether `block` outbound is unconditional or conditional on active rules.
- Detailed Rust-side validation strictness and UI micro-copy.
- Verify the real xray asset path on a build machine; operator smoke test with
  real binary remains required (CI has no bundled binary).

## Verification plan (for the later implementation plan)

- Rust unit tests: config emits correct rules/outbounds for proxy/direct/block;
  private bypass precedes user rules; probe config unchanged; no active profile
  → unchanged connection shape; enabled/disabled rules filtered; active
  activation clears others; disabled profile can't activate; cascade delete.
- Command tests: CRUD + validation errors + command-name surface.
- TS: Zod contract tests for new payloads; Vitest component test for basic
  master/detail selection and outbound values if the repo has a UI test setup.
- Baseline before implementation: `pnpm test`; `cargo test` with test-only
  `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` and
  `-- --test-threads=1`. Both must be green on the clean worktree first.
