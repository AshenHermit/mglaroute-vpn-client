# Probe UI: event-driven row state

## Intent

`Run test` currently calls `setProbeStarted(profiles.map(...))`, which marks every
visible server as `Testing...` on click. That optimistic batch ignores
`test_concurrency`; the Rust cycle runs `buffer_unordered(test_concurrency)`, so
with `1` (or any value) the UI lies about how many cores are actually running.
Terminal results also do not reach the table until `probe_run` resolves and
`profile_list` is re-fetched.

Goal: highlighted rows are exactly those whose `profileId` arrived in a
`probe-server` event with `status: "testing"`, regardless of the concurrency
value. No count-based, slice-based or optimistic batch selection is allowed:
one event for `p-7` highlights only row `p-7`. Terminal results update the row
immediately.

## Goals

- Spinner and `Stop test` still appear on click, before any backend event.
- A row gets the `Testing...` / `…` state only on a `testing` event; it is removed
  on the matching terminal event.
- `done` / `failed` immediately patch `latencyMs` and `reachable` in
  `state.profiles`; the table re-sorts from those values at once.
- `cancelled` removes the highlight but does not overwrite the previous result.
- The final `profile_list` re-fetch after `probe_run` stays authoritative.

## Non-goals

- No Rust or contract changes.
- No synthetic `checkedAt`: the backend writes it to SQLite only after the whole
  cycle, so a client-side timestamp would be bogus. The table does not display
  it.
- No new per-service columns in the table.
- No change to backend persistence or scheduling.

## Constraints

- `ProbeServerEvent` carries `profileId`, `status`, `latencyMs`, `serviceResults`.
- `Profile` carries `latencyMs`, `reachable`, `checkedAt`, `sortIndex`.
- Backend persists all samples in `persist_probe_samples` after the cycle, so a
  per-result `profile_list` refetch would return stale data.
- `test_concurrency` defaults to 10, clamped to 1..=20. The UI should not read it:
  events are the source of truth for which core is running.

## Approaches

1. **Frontend-only patch (chosen).** Keep optimistic `probeTesting` for
   spinner/Stop, but start with an empty `probeTestingIds`. Add an id on
   `testing`; remove it and patch `latencyMs`/`reachable` on `done`/`failed`.
   Minimal diff, TS + tests only. Cost: `checkedAt` updates at final refetch.
2. **Richer event contract.** Add `checkedAt`/`reachable` to the Rust event and
   contracts. More exact, but touches Rust, contracts and requires backend
   rebuild. Rejected as disproportionate for a UI display fix.
3. **Refetch `profile_list` per terminal event.** Rejected: persistence happens
   only at cycle end, so mid-cycle refetch reads stale rows.

## Implementation outline

- `app.store.ts` `setProbeStarted(ids)`: set `probeTesting: true`,
  `probeTestingIds: []`, reset counters and cancellation; keep `probeTotal:
  ids.length` for the `tested / total` counter.
- `app.store.ts` `setProbeServer`: on `testing`, add the id; on `done`/`failed`,
  remove it and map the matching profile to new `latencyMs`/`reachable`; on
  `cancelled`, only remove it. `reachable` for `done` is `serviceResults.length >
  0 && serviceResults.every(r => r.reachable)`; `failed` forces `false`.
- `Servers.tsx` keeps the existing click/finally flow; no concurrency logic.
- Tests: one `testing` event highlights exactly one row even when the candidate
  list has many; terminal event patches the row before `probe_run` settles;
  spinner/Stop still render on click; `probe_busy` still clears in `finally`.

## Left to the builder

Verify with vitest and a fresh `pnpm build`; no live smoke by the agent
(system-proxy side effects are operator-only).
