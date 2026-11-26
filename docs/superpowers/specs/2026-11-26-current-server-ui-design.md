# Current server UI design

## Intent

Operator wants one clear notion of the *active server* instead of two
separate "selected" / "connected" concepts. A power button in each server row
sets that server active; the Dashboard always shows the active server. The
global connection (xray + system proxy) stays controlled by `Connect` in the
connection panel. If the VPN is already connected, pressing power on another
row switches the running connection to that server.

## Goals

- Add `lucide-react`; use a `Power` icon for the per-row action.
- Row action sets `selectedProfileId` (session-only active target).
- If `ConnectionState.status === "connected"`, the same row action calls
  `connection_connect({ profileId })` to switch the live VPN to that server.
- Dashboard shows the active server: `ConnectionState.profileId` when
  connected, otherwise `selectedProfileId`.
- Visual states for the row power button:
  - inactive row: neutral/white outline icon;
  - active but VPN disconnected: gray icon;
  - active and VPN connected: green icon.
- Remove row-click selection: clicking the row body no longer changes active
  server. Only the power button does.

## Non-goals

- No Rust or contract changes. `connection_connect` already accepts an
  explicit `profileId` and supports switching from a connected state.
- Do not remove or replace the global `Connect` / `Disconnect` panel.
- Do not persist `selectedProfileId` across app restarts. After a restart with
  no connection, no row is active; with a live connection,
  `ConnectionState.profileId` is authoritative.
- Do not auto-connect on app start or on row selection while disconnected.
- No new per-service columns or probe-behavior changes.

## Approaches

1. **Show selected and connected separately.** Rejected by operator: "нет
   разделения на выбранный и подключенный".
2. **Power only selects; Connect always starts/stops.** Earlier clarification,
   but incomplete: operator also wants power to switch an already-running VPN.
3. **Chosen — one active server, context-aware power.** Power sets the active
   target; when connected it also switches the VPN. Dashboard shows one server
   whose icon colour reflects connection state. Matches operator's final
   description exactly and needs no backend work.

## Decisions

- Active id = `connection.profileId ?? selectedProfileId`. When connected,
  backend state wins; when disconnected, the session selection wins.
- Power click handler: `setSelectedProfileId(profile.id)`; if
  `connection.status === "connected"`, also `invoke("connection_connect", {
  profileId: profile.id })`. Errors surface through the existing notice path;
  no local optimistic "connected" state.
- ServerTable receives `connectedProfileId` from `useConnectionStore` and
  renders the icon state from it plus `selectedId`.
- Add `lucide-react` as a desktop dependency. Keep existing UI components.

## Left to the builder

- Update `Servers.tsx`, `ServerTable.tsx`, `Dashboard.tsx`, tests.
- Install `lucide-react`, run tests and `pnpm build`, commit.
