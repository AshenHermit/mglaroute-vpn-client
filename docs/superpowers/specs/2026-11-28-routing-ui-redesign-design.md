# Routing UI redesign — design

Status: proposed  
Date: 2026-11-28

## Intent

The routing feature landed with a two-pane master/detail UI built from plain
tables and local components whose `Switch` and `Tabs` styling looks wrong.
The operator wants the whole routing settings surface rebuilt: a vertical list
of standalone profile cards with a single activation switch, collapsible rule
panels with card-style rules, a rule editor modal, and an animated `Switch` /
`Tabs` / `Collapsible` from `@animate-ui/react` (Radix variants). Tabs must have
a visible indicator. Save inside a profile applies the whole rule list at once.

## Goals

- Replace `RoutingEditor` completely: no leftover two-pane tables or inline
  editor.
- `Add Profile` button opens a dialog with a name field; confirm creates a
  profile and leaves it available in the list.
- Profile list is a vertical stack of individual cards; each card shows the
  profile name with a pencil icon that switches it into inline rename mode, an
  activation `Switch`, a `Collapsible.Trigger`, and a `Collapsible.Panel`
  containing that profile's rules as cards.
- Activating a profile via `Switch` clears `is_active` on all others. Deactivating
  the last active profile is allowed and leaves no active profile.
- Remove the profile `enabled` concept entirely: no UI control, no DB column,
  no contract field, no Rust command parameter. Only `is_active` exists.
  `routing_profile_update` stays with `{id, name}` only, driven by the pencil
  icon. `routing_profile_activate` and `routing_profile_remove` stay unchanged.
- Rules are edited in a modal dialog. The profile panel has a `Save` button
  that replaces the profile's entire rule list in one batch.
- Migrate all app `Switch` and `Tabs` usages to `@animate-ui/react/primitives/radix-ui`
  (`Switch.Root`/`Switch.Thumb`, `Tabs.Root`/`Tabs.List`/`Tabs.Trigger`/
  `Tabs.Content`) with a visible tabs indicator. Add
  `Collapsible` from the same package for profile cards.
- Keep all existing routing behavior: config generation, private bypass,
  proxy exclusions, geosite/geoip prefixes, blackhole outbound, probe isolation.

## Non-goals

- No new rule fields (port/protocol/network/inboundTag/process/DNS).
- No drag-and-drop; up/down reorder stays inside the rule editor modal if kept.
- No import/export of third-party routing formats.
- No change to connection/supervisor/platform code.
- No visual redesign of unrelated pages beyond shared Switches/Tabs.

## Anti-goals (failure even if it compiles)

- Two competing "on" flags for a profile (`enabled` + `is_active`) confusing the
  operator.
- A Save that partially applies a rule list.
- A tabs row without an indicator.
- A routing editor that still renders rule tables.

## Constraints

- Stack fixed: Tauri v2 + Rust/`rusqlite`, React 18 + TypeScript strict,
  shadcn-style local wrappers + Tailwind 3, Zustand 5 with `persist` only for
  non-secret UI prefs. Durable data in SQLite.
- Use `@animate-ui/react/primitives/radix-ui` (operator-corrected path), not
  `base-ui`. Add `@animate-ui/react` + `motion`; shadcn and Animate UI CLI add
  `switch`, `tabs`, `collapsible` into `apps/desktop/src/components/ui`.
- Existing schema is `PRAGMA user_version = 2`. Removing `enabled` from
  `routing_profiles` requires a deliberate v2 → v3 migration; `migrate` in
  `storage/mod.rs` gains the new branch.
- `lib.rs::command_list!` registers every Tauri command and
  `commands/mod.rs::command_names_registered()` asserts the sorted surface.
  Removing `routing_profile_update(enabled)` and adding a batch replace command
  changes that list and its test.
- Rust emits `camelCase`; Zod contracts in `packages/contracts/src/schemas.ts`
  must match exactly.
- Caveman-mode terse output; exact identifiers preserved.

## Approaches considered

### A — Patch the existing RoutingEditor in place

Keep tables and inline editor, restyle with new primitives. Rejected: operator
explicitly said to throw the current interface away.

### B — Rebuild RoutingEditor as card list + collapsible panels + modal (chosen)

Rewrite the feature component with profile cards, modal rule editor, and one
batch-replace command. Trade-off: touches SQLite migration, commands, contracts,
tests, and shared UI primitives — larger diff but matches the requested UX and
removes the duplicated profile flag.

### C — UI-only redesign, keep `enabled` hidden in DB

Leave the Rust/contract/storage layer untouched and hide `enabled`. Rejected by
operator: "полностью убрать enabled".

## Decisions (confirmed with operator)

1. Remove profile `enabled` completely (DB, commands, contract, UI). `is_active`
   is the only profile switch.
2. `routing_rules_replace(profileId, rules)` performs full replacement in one
   SQLite transaction: delete all rows for the profile, insert the submitted
   rules with fresh IDs and sequential `sort`. Rule IDs change on every Save;
   this is accepted.
3. Deactivating the last active profile is allowed; fallback is built-in bypass
   + `proxyExclusions` + catch-all proxy (current no-active-profile behavior).
4. Animate UI primitives: Radix variants only (`@animate-ui/react/primitives/radix-ui`).
5. Switch/Tabs replacement is global across the app; Collapsible is added for
   profile cards. Tabs keep a visible highlight/indicator.
6. Rule modal edits one rule at a time into local panel state; `Save` in the
   panel persists the whole list atomically. Builder calls: no dirty tracking —
   Save always replaces the list; per-rule `enabled` toggle stays in the modal
   and on the rule card; up/down reorder stays in the modal.

## Builder left to decide

- Exact migration SQL for v2 → v3 (`ALTER TABLE` limitations in SQLite may need
  table rebuild); all other columns and rows survive unchanged.
- Exact Animate UI CLI output paths and whether the CLI overwrites existing
  `components/ui/switch.tsx` / `tabs.tsx`; reconcile with local wrappers.
- Test split between contracts, routing repo, command surface, and UI tests;
  all existing suites must stay green.
