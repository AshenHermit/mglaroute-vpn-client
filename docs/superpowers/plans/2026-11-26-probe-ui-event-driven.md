# Probe UI Event-Driven Rows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Highlight exactly the servers whose `probe-server` `testing` event arrived, and patch result columns immediately on `done`/`failed`.

**Architecture:** Frontend-only change. `setProbeStarted` keeps optimistic spinner/Stop state but starts with an empty testing-id set. `setProbeServer` owns row membership: add on `testing`, remove on terminal, patch profile values on `done`/`failed`. Final `profile_list` re-fetch remains authoritative. No Rust or contract changes.

**Tech Stack:** TypeScript, React, Zustand, Vitest, Testing Library, Zod contracts.

**Spec:** `docs/superpowers/specs/2026-11-26-probe-ui-event-driven-design.md`

## Global Constraints

- No change to `test_concurrency` handling: backend keeps `buffer_unordered(test_concurrency)`, default 10, clamp 1..=20.
- The UI must not read `test_concurrency` to decide which rows are highlighted.
- One `testing` event for `profileId: "p-7"` highlights only row `p-7`.
- Do not fabricate `checkedAt`; backend writes it at cycle end.
- Do not change Rust, `packages/contracts`, or probe persistence.
- Keep optimistic `probeTesting` on click so spinner and `Stop test` always appear.
- `cancelled` removes the highlight but never overwrites a previous profile result.
- Commits use Conventional Commits, subject ≤50 chars where possible.

## Review Focus

- `test_concurrency = 1` with 20 candidates: after one `testing` event, exactly one row highlighted — pinned by Task 1 test "highlights only the profile from a testing event".
- Terminal result arrives while `probe_run` is still pending: row `latencyMs`/`reachable` patched before the promise settles — pinned by Task 2 test "patches done and failed rows before probe_run settles".
- `failed` event with `latencyMs: null`: row immediately shows failed state — pinned by Task 2 test "patches done and failed rows before probe_run settles".
- `cancelled` event: highlight removed, previous values preserved — pinned by Task 2 test "cancelled removes highlight without overwriting the row".
- Fast `probe_busy` with no per-server events: spinner/Stop still clear in `finally`, no stuck rows — pinned by Task 1 existing test "clears testing state when probe_run fails".

---

### Task 1: Event-driven highlighted ids

**Files:**
- Modify: `apps/desktop/src/stores/app.store.ts:85-95`
- Modify: `apps/desktop/src/pages/Servers.test.tsx:83-98`
- Test: `apps/desktop/src/pages/Servers.test.tsx`

**Interfaces:**
- Consumes: `setProbeServer(event: ProbeServerEvent)` (unchanged).
- Produces: `setProbeStarted(ids: string[])` with identical signature but new semantics — `probeTestingIds` starts `[]`, `probeTotal = ids.length` retained. `setProbeSettled()` unchanged.

- [ ] **Step 1: Write the failing tests**

In `apps/desktop/src/pages/Servers.test.tsx`, add a second `Profile` constant `otherServer` with `id: "p-2"`, `serverId: "p-2"`, otherwise identical to `server`, and include it in `beforeEach` `useAppStore.setState` and `profile_list` mock. Change the existing test "shows testing state on click before any backend event arrives" assertions to:

```ts
expect(useAppStore.getState().probeTesting).toBe(true);
expect(useAppStore.getState().probeTestingIds).toEqual([]);
expect(screen.getByRole("button", { name: "Stop test" })).not.toBeNull();
expect(
  screen.getAllByTestId("server-row").some((row) =>
    row.textContent?.includes("Testing..."),
  ),
).toBe(false);
```

Add a new test "highlights only the profile from a testing event":

```ts
act(() => {
  useAppStore.getState().setProbeServer({
    profileId: "p-2",
    status: "testing",
    latencyMs: null,
    serviceResults: [],
  });
});
const rows = screen.getAllByTestId("server-row");
expect(
  rows.find((row) => row.getAttribute("data-profile-id") === "p-2")?.textContent,
).toContain("Testing...");
expect(
  rows.find((row) => row.getAttribute("data-profile-id") === "p-1")?.textContent,
).not.toContain("Testing...");
```

Import `act` from `@testing-library/react` at the top.

- [ ] **Step 2: Run tests to verify RED**

Run (from `apps/desktop`):

```bash
pnpm exec vitest run src/pages/Servers.test.tsx
```

Expected: FAIL — `probeTestingIds` is `["p-1","p-2"]` after click, and no row is uniquely selected after a single `testing` event.

- [ ] **Step 3: Change `setProbeStarted`**

In `apps/desktop/src/stores/app.store.ts`, replace `probeTestingIds: ids` with `probeTestingIds: []`. Update the comment to say the spinner/Stop are optimistic, while row membership is owned strictly by `probe-server` events so any `test_concurrency` value is reflected correctly. Keep `probeTotal: ids.length`.

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
pnpm exec vitest run src/pages/Servers.test.tsx
```

Expected: PASS, all tests in the file green.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/stores/app.store.ts apps/desktop/src/pages/Servers.test.tsx
git commit -m "fix(probe): highlight rows only from testing events"
```

---

### Task 2: Apply terminal results to rows immediately

**Files:**
- Modify: `apps/desktop/src/stores/app.store.ts:68-84`
- Test: `apps/desktop/src/stores/app.store.test.tsx:161-218`

**Interfaces:**
- Consumes: `ProbeServerEvent` from `ProbeServerEventSchema`; `Profile` from `@mglaroute/contracts`.
- Produces: `setProbeServer` also patches `state.profiles`; no new store action.

- [ ] **Step 1: Write the failing tests**

In `apps/desktop/src/stores/app.store.test.tsx`, extend the existing test "tracks live server statuses and progress counts from probe events": after the `done` event with `latencyMs: 12` and `serviceResults: []`, add:

```ts
expect(useAppStore.getState().profiles[0]?.latencyMs).toBe(12);
expect(useAppStore.getState().profiles[0]?.reachable).toBe(false);
```

Add a new test "patches done and failed rows before probe_run settles" that renders `<App />`, sends one `testing` event for `p-1`, then a `done` event with `latencyMs: 50` and `serviceResults: [{ serviceProfileId: "default-google", reachable: true }]`, asserts `profiles[0]` has `latencyMs: 50`, `reachable: true`, `probeTestingIds: []`; then sends a `failed` event for `p-1` with `latencyMs: null`, `serviceResults: []`, asserts `profiles[0]` has `latencyMs: null`, `reachable: false`.

Add a new test "cancelled removes highlight without overwriting the row" that renders `<App />`, first patches the row via a `done` event (`latencyMs: 30`, one reachable service), then sends `testing`, then `cancelled` for `p-1`, and asserts `probeTestingIds` is `[]` while `profiles[0]` still has `latencyMs: 30` and `reachable: true`.

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
pnpm exec vitest run src/stores/app.store.test.tsx
```

Expected: FAIL — `profiles[0]` still has the initial `latencyMs: null` and `reachable: false`.

- [ ] **Step 3: Patch profiles in `setProbeServer`**

In `apps/desktop/src/stores/app.store.ts`, in the terminal branch of `setProbeServer` (after `ids.delete(event.profileId)`), keep existing `probeTestingIds`/`probeTesting` return and add a `profiles` mapping:

```ts
const done = event.status === "done";
const latencyMs = done ? event.latencyMs : null;
const reachable =
  event.status === "failed"
    ? false
    : event.serviceResults.length > 0 &&
      event.serviceResults.every((result) => result.reachable);
const profiles = state.profiles.map((profile) =>
  profile.id === event.profileId ? { ...profile, latencyMs, reachable } : profile,
);
```

Return `profiles` only for `done`/`failed`; for `cancelled` return the id/testing update alone. Preserve the `starting && state.probeCancelled` reset behavior in the `testing` branch.

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
pnpm exec vitest run src/stores/app.store.test.tsx src/pages/Servers.test.tsx
```

Expected: PASS, both files green.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/stores/app.store.ts apps/desktop/src/stores/app.store.test.tsx
git commit -m "fix(probe): apply results to rows on terminal events"
```

---

### Task 3: Full verification and docs

**Files:**
- Modify: `AGENTS.md` (only if it still claims per-server events are the sole source of UI progress)
- Build: `apps/desktop/dist` via `pnpm build` (generated, not committed)

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: verified working tree and built frontend for the operator.

- [ ] **Step 1: Update docs if needed**

Read `AGENTS.md`. If it says per-server events are the only source of UI progress, edit it to say: spinner/Stop are optimistic on click; highlighted rows and immediate result columns come from `probe-server` events; final `profile_list` remains authoritative.

- [ ] **Step 2: Run full frontend suite**

From worktree root:

```bash
pnpm exec nx run-many --target=test --skip-nx-cache
```

Expected: `contracts` 5/5 and `desktop` green (currently 37 tests plus the new ones).

- [ ] **Step 3: Rebuild frontend**

From `apps/desktop`:

```bash
pnpm build
```

Expected: `tsc --noEmit` and Vite build clean except pre-existing Zod/Rollup comment warnings.

- [ ] **Step 4: Verify built asset contains the current UI**

```bash
python -c "from pathlib import Path; p=next(Path('apps/desktop/dist/assets').glob('index-*.js')); s=p.read_text(encoding='utf-8'); print(p.name); [print(repr(x), x in s) for x in ['Testing...','Stop test','probe-server','animate-spin']]"
```

Expected: new asset name and all four strings present. Note the asset name in the final report.

- [ ] **Step 5: Commit docs if changed, confirm clean tree**

```bash
git add AGENTS.md
git commit -m "docs: clarify event-driven probe row state"
git status --short
```

Expected: empty status (or only generated files ignored by git).
